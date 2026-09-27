package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/chart/common/util"
	"helm.sh/helm/v4/pkg/cli"
)

// The frozen roster, as landing.py sweep_plan writes it and read_roster checks it.
const (
	rosterHeader = "chart\toverride_id\toverride\toverride_sha256\ttransport\tkube_version\tbase_schema\tcand_schema_sha256\tbase_schema_sha256"
	schemaFile   = "values.schema.json"
)

var (
	chartName   = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]*$`)
	decimal     = regexp.MustCompile(`^[0-9]+$`)
	hex64       = regexp.MustCompile(`^[0-9a-f]{64}$`)
	kubeVersion = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+$`)
	cells       = [3]string{"base", "cand", "plain"}
)

type rosterRow struct {
	Chart, ID, Name, OverrideSHA, Transport, KubeVersion, BaseSchema, CandSHA, BaseSHA string
}

// chartRun is one chart of the roster: its rows, verified copies and results.
type chartRun struct {
	name    string
	rows    []rosterRow
	digest  string            // content digest of the prepared copy, without the root schema
	schemas map[string][]byte // cell -> root values.schema.json bytes; absent for plain and a schema-less base
	// cacheable is false when identical invocations may reach different
	// verdicts (chartCacheability), with the first reason.
	cacheable   bool
	uncacheable string

	mu        sync.Mutex
	remaining int
	outcomes  map[[2]int]outcome // (row, cell) -> outcome
	start     time.Time
}

type outcome struct {
	rc         int
	class      string
	hit        bool
	attributed bool
	elapsed    time.Duration
}

type sweeper struct {
	work, plan string
	id         identity
	cache      verdictCache
	sink       *logSink
	stderr     io.Writer
	compile    chan struct{} // one schema compile at a time

	mu       sync.Mutex
	warmed   map[[32]byte]bool
	failures []error
}

func cmdSweep(args []string, stderr io.Writer) int {
	flags := flag.NewFlagSet("sweep", flag.ContinueOnError)
	flags.SetOutput(stderr)
	roster := flags.String("roster", "", "frozen roster.tsv (its plan/ directory is its sibling)")
	rosterSHA := flags.String("roster-sha256", "", "the roster's sha256 as sweep-plan printed it")
	work := flags.String("work", "", "absolute directory holding <chart>/{base,cand,plain}")
	jobs := flags.Int("jobs", 4, "concurrent cells")
	cacheDir := flags.String("cache", "", "verdict cache directory")
	clearEnv := flags.Bool("helm-env-clear", false, "required: clear the Helm environment before the workers start")
	if err := flags.Parse(args); err != nil || flags.NArg() != 0 || *roster == "" || *work == "" || *cacheDir == "" ||
		!hex64.MatchString(*rosterSHA) || *jobs < 1 || !*clearEnv || !filepath.IsAbs(*work) || filepath.Clean(*work) != *work {
		fmt.Fprintln(stderr, "usage: helmsweep sweep --roster FILE --roster-sha256 HEX --work ABSDIR [--jobs N] --cache DIR --helm-env-clear")
		return exitRefused
	}
	if err := requireCleanEnv(); err != nil {
		fmt.Fprintln(stderr, "helmsweep: refused:", err)
		return exitRefused
	}
	if err := unsupportedConfiguration(cli.New()); err != nil {
		fmt.Fprintln(stderr, "helmsweep: refused:", err)
		return exitRefused
	}
	id, err := buildIdentity()
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep: refused:", err)
		return exitRefused
	}
	if id.HelmBuild != helmReleaseBuild {
		fmt.Fprintf(stderr, "helmsweep: refused: Helm build %q is not the pinned release %q\n", id.HelmBuild, helmReleaseBuild)
		return exitRefused
	}
	s := &sweeper{
		work: *work, plan: filepath.Join(filepath.Dir(*roster), "plan"), id: id, cache: verdictCache{*cacheDir},
		stderr: stderr, compile: make(chan struct{}, 1), warmed: map[[32]byte]bool{},
	}
	charts, err := s.prepare(*roster, *rosterSHA)
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep: refused:", err)
		return exitRefused
	}
	s.sink = installLogSink(stderr)
	s.run(charts, *jobs)
	if len(s.failures) > 0 {
		fmt.Fprintln(stderr, "helmsweep:", errors.Join(s.failures...))
		return exitHarness
	}
	return 0
}

// prepare verifies the roster against its frozen hash, every override and
// schema in the plan against the roster, and every prepared copy: plain has
// no values.schema.json anywhere, base and cand are plain plus exactly the
// planned root schema, and no chart has outputs yet.
func (s *sweeper) prepare(rosterPath, rosterSHA string) ([]*chartRun, error) {
	text, err := os.ReadFile(rosterPath)
	if err != nil {
		return nil, err
	}
	if got := fmt.Sprintf("%x", sha256.Sum256(text)); got != rosterSHA {
		return nil, fmt.Errorf("roster %s has sha256 %s, not the frozen %s", rosterPath, got, rosterSHA)
	}
	lines := strings.Split(string(text), "\n")
	if len(lines) < 3 || lines[0] != rosterHeader || lines[len(lines)-1] != "" {
		return nil, errors.New("roster is empty or malformed")
	}
	var charts []*chartRun
	byName := map[string]*chartRun{}
	for _, line := range lines[1 : len(lines)-1] {
		f := strings.Split(line, "\t")
		if len(f) != 9 {
			return nil, fmt.Errorf("malformed roster row %q", line)
		}
		row := rosterRow{f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8]}
		base := (row.BaseSchema == "baseline" && hex64.MatchString(row.BaseSHA)) || (row.BaseSchema == "absent" && row.BaseSHA == "-")
		if !chartName.MatchString(row.Chart) || !decimal.MatchString(row.ID) || row.Name == "" || !hex64.MatchString(row.OverrideSHA) ||
			row.Transport != "values-file" || !kubeVersion.MatchString(row.KubeVersion) || !hex64.MatchString(row.CandSHA) || !base {
			return nil, fmt.Errorf("malformed roster row %q", line)
		}
		c := byName[row.Chart]
		if c == nil {
			c = &chartRun{name: row.Chart, outcomes: map[[2]int]outcome{}}
			byName[row.Chart] = c
			charts = append(charts, c)
		}
		c.rows = append(c.rows, row)
	}
	for _, c := range charts {
		if err := s.verifyChart(c); err != nil {
			return nil, fmt.Errorf("%s: %w", c.name, err)
		}
	}
	return charts, nil
}

func (s *sweeper) verifyChart(c *chartRun) error {
	plan := filepath.Join(s.plan, c.name)
	first := c.rows[0]
	planned := map[string]string{"cand": first.CandSHA, "base": first.BaseSHA}
	for _, row := range c.rows {
		if row.CandSHA != first.CandSHA || row.BaseSHA != first.BaseSHA || row.BaseSchema != first.BaseSchema {
			return errors.New("rows disagree about the chart's schemas")
		}
		sum, err := fileSHA256(filepath.Join(plan, "ov", row.ID+".json"))
		if err != nil || sum != row.OverrideSHA {
			return fmt.Errorf("override %s does not match the roster (%v)", row.ID, err)
		}
		if name, err := os.ReadFile(filepath.Join(plan, "ov", row.ID+".name")); err != nil || string(name) != row.Name {
			return fmt.Errorf("override %s name does not match the roster", row.ID)
		}
	}
	for cell, want := range planned {
		sum, err := fileSHA256(filepath.Join(plan, cell+".schema.json"))
		if (want == "-" && !errors.Is(err, fs.ErrNotExist)) || (want != "-" && sum != want) {
			return fmt.Errorf("plan %s schema does not match the roster (%v)", cell, err)
		}
	}

	dir := filepath.Join(s.work, c.name)
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, e := range entries {
		if e.Name() != "base" && e.Name() != "cand" && e.Name() != "plain" {
			return fmt.Errorf("%s already holds %s", dir, e.Name())
		}
	}
	digest, schema, err := copyDigest(filepath.Join(dir, "plain"))
	if err != nil {
		return err
	}
	if schema != nil {
		return errors.New("the plain copy carries a values.schema.json")
	}
	c.digest, c.schemas = digest, map[string][]byte{}
	if c.cacheable, c.uncacheable, err = chartCacheability(filepath.Join(dir, "plain")); err != nil {
		return err
	}
	for cell, want := range planned {
		digest, schema, err := copyDigest(filepath.Join(dir, cell))
		if err != nil {
			return err
		}
		if digest != c.digest {
			return fmt.Errorf("the %s copy is not the plain copy plus its schema", cell)
		}
		if (want == "-") != (schema == nil) || (schema != nil && fmt.Sprintf("%x", sha256.Sum256(schema)) != want) {
			return fmt.Errorf("the %s copy's values.schema.json does not match the roster", cell)
		}
		if schema != nil {
			if err := localReferencesOnly(schema); err != nil {
				return fmt.Errorf("%s schema: %w", cell, err)
			}
			c.schemas[cell] = schema
		}
	}
	c.remaining = len(c.rows) * len(cells)
	return nil
}

// copyDigest hashes a prepared chart copy: every entry's relative path, type,
// permission bits and file bytes, in lexical order, leaving out the root
// values.schema.json, which it returns. A symlink, special file or nested
// values.schema.json is refused.
func copyDigest(root string) (string, []byte, error) {
	h := sha256.New()
	var schema []byte
	err := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		switch {
		case rel == schemaFile && info.Mode().IsRegular():
			schema, err = os.ReadFile(path)
			return err
		case d.Name() == schemaFile:
			return fmt.Errorf("%s is a values.schema.json below the chart root", path)
		case info.IsDir():
			fmt.Fprintf(h, "d %o %q\n", info.Mode().Perm(), rel)
		case info.Mode().IsRegular():
			sum, err := fileSHA256(path)
			if err != nil {
				return err
			}
			fmt.Fprintf(h, "f %o %q %s\n", info.Mode().Perm(), rel, sum)
		default:
			return fmt.Errorf("%s is a symlink or special file", path)
		}
		return nil
	})
	return fmt.Sprintf("%x", h.Sum(nil)), schema, err
}

// localReferencesOnly refuses a schema whose compile could load a resource:
// a $ref, $dynamicRef or $recursiveRef not starting with "#", or a $schema
// that is not a draft jsonschema/v6 knows. The sweep makes no network
// requests and schema bytes alone cannot identify external content.
func localReferencesOnly(schema []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(schema))
	decoder.UseNumber()
	var doc any
	if err := decoder.Decode(&doc); err != nil {
		return nil // Helm reports the parse error itself
	}
	var walk func(v any) error
	walk = func(v any) error {
		switch v := v.(type) {
		case map[string]any:
			for key, child := range v {
				text, isString := child.(string)
				switch {
				case !isString:
				case (key == "$ref" || key == "$dynamicRef" || key == "$recursiveRef") && !strings.HasPrefix(text, "#"):
					return fmt.Errorf("nonlocal %s %q", key, text)
				case key == "$schema" && !knownDraft(text):
					return fmt.Errorf("unknown $schema %q", text)
				}
				if err := walk(child); err != nil {
					return err
				}
			}
		case []any:
			for _, child := range v {
				if err := walk(child); err != nil {
					return err
				}
			}
		}
		return nil
	}
	return walk(doc)
}

// knownDraft mirrors jsonschema/v6 draftFromURL (draft.go) for an empty fragment.
func knownDraft(url string) bool {
	url = strings.TrimSuffix(url, "#")
	if rest, ok := strings.CutPrefix(url, "http://"); ok {
		url = rest
	} else {
		url = strings.TrimPrefix(url, "https://")
	}
	switch url {
	case "json-schema.org/schema", "json-schema.org/draft/2020-12/schema", "json-schema.org/draft/2019-09/schema",
		"json-schema.org/draft-07/schema", "json-schema.org/draft-06/schema", "json-schema.org/draft-04/schema":
		return true
	}
	return false
}

type task struct {
	chart *chartRun
	row   int
	cell  int
}

// run executes every cell on a bounded pool, in roster order.
func (s *sweeper) run(charts []*chartRun, jobs int) {
	tasks := make(chan task)
	var wg sync.WaitGroup
	for range jobs {
		wg.Go(func() {
			for t := range tasks {
				s.execute(t)
			}
		})
	}
	started := time.Now()
	for _, c := range charts {
		for row := range c.rows {
			for cell := range cells {
				tasks <- task{c, row, cell}
			}
		}
	}
	close(tasks)
	wg.Wait()
	compiles, memoHits := util.CompiledSchemaMemoStats()
	fmt.Fprintf(s.stderr, "helmsweep: charts=%d wall=%s schema-compiles=%d memo-hits=%d failures=%d\n",
		len(charts), time.Since(started).Round(time.Millisecond), compiles, memoHits, len(s.failures))
}

func (s *sweeper) fail(err error) {
	s.mu.Lock()
	s.failures = append(s.failures, err)
	s.mu.Unlock()
}

// execute runs one cell: a verified cache hit, else Helm, re-run exclusively
// when a concurrent log record could have changed its class.
func (s *sweeper) execute(t task) {
	c, row, cell := t.chart, t.chart.rows[t.row], cells[t.cell]
	c.mu.Lock()
	if c.start.IsZero() {
		c.start = time.Now()
	}
	c.mu.Unlock()
	mode := "lint"
	if cell == "plain" {
		mode = "template"
	}
	chartPath := filepath.Join(s.work, c.name, cell)
	valuesFile := filepath.Join(s.plan, c.name, "ov", row.ID+".json")
	schema := c.schemas[cell]
	schemaSHA := "none"
	if schema != nil {
		schemaSHA = fmt.Sprintf("%x", sha256.Sum256(schema))
	}
	key := cacheKey{
		Format: cacheFormat, Executable: s.id.Executable, Go: s.id.Go, Helm: s.id.Helm, HelmBuild: s.id.HelmBuild, JSONSchema: s.id.JSONSchema,
		Patch: s.id.Patch, Environment: cacheEnv, Mode: mode, KubeVersion: row.KubeVersion, Chart: c.digest,
		Schema: schemaSHA, Override: row.OverrideSHA,
	}

	start := time.Now()
	o := outcome{attributed: true}
	var log []byte
	if c.cacheable {
		o.rc, o.class, log, o.hit = s.cache.lookup(key, chartPath)
	}
	if !o.hit {
		if schema != nil {
			s.warm(schema)
		}
		runOne := func() (int, []byte) { return runCell(mode, chartPath, row.KubeVersion, valuesFile) }
		o.rc, log, o.attributed = s.sink.shared(runOne)
		if !o.attributed {
			o.rc, log = s.sink.exclusive(runOne)
		}
		o.class = classify(mode, o.rc, log)
		// An uncacheable chart's verdicts are never stored: identical inputs may reach another.
		if c.cacheable {
			if err := s.cache.store(key, o.rc, o.class, chartPath, log); err != nil {
				fmt.Fprintf(s.stderr, "helmsweep: %s/%s.%s: not cached: %v\n", c.name, cell, row.ID, err)
			}
		}
	}
	o.elapsed = time.Since(start)
	if err := os.WriteFile(filepath.Join(s.work, c.name, cell+"."+row.ID+".log"), log, 0o644); err != nil {
		s.fail(err)
	}

	c.mu.Lock()
	c.outcomes[[2]int{t.row, t.cell}] = o
	c.remaining--
	done := c.remaining == 0
	c.mu.Unlock()
	if done {
		s.finish(c)
	}
}

// warm compiles a schema into Helm's memo, one compile at a time, through
// Helm's own validation function; the verdict on empty values is not used.
// It holds the log gate shared, so no record of it reaches an exclusive cell.
func (s *sweeper) warm(schema []byte) {
	sum := sha256.Sum256(schema)
	s.mu.Lock()
	done := s.warmed[sum]
	s.mu.Unlock()
	if done {
		return
	}
	s.compile <- struct{}{}
	s.sink.gate.RLock()
	_ = util.ValidateAgainstSingleSchema(common.Values{}, schema)
	s.sink.gate.RUnlock()
	<-s.compile
	s.mu.Lock()
	s.warmed[sum] = true
	s.mu.Unlock()
}

// finish writes the chart's rows.tsv (sweep-one.sh's columns, roster order)
// and cells.tsv, and drops its schemas from the memo.
func (s *sweeper) finish(c *chartRun) {
	var rows, detail bytes.Buffer
	detail.WriteString("chart\toverride_id\tcell\trc\tclass\tcache\tattributed\tmillis\tcacheable\n")
	cacheable := map[bool]string{true: "yes", false: "no"}[c.cacheable]
	hits := 0
	for i, row := range c.rows {
		rc := [3]int{}
		for j, cell := range cells {
			o := c.outcomes[[2]int{i, j}]
			rc[j] = o.rc
			cache := "miss"
			if o.hit {
				cache = "hit"
				hits++
			}
			fmt.Fprintf(&detail, "%s\t%s\t%s\t%d\t%s\t%s\t%t\t%d\t%s\n", c.name, row.ID, cell, o.rc, o.class, cache, o.attributed, o.elapsed.Milliseconds(), cacheable)
		}
		fmt.Fprintf(&rows, "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%d\t%d\t%d\n", c.name, row.ID, row.Name, row.OverrideSHA, row.Transport,
			row.KubeVersion, row.BaseSchema, rc[0], rc[1], rc[2])
	}
	dir := filepath.Join(s.work, c.name)
	if err := os.WriteFile(filepath.Join(dir, "cells.tsv"), detail.Bytes(), 0o644); err != nil {
		s.fail(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "rows.tsv"), rows.Bytes(), 0o644); err != nil {
		s.fail(err)
	}
	for _, schema := range c.schemas {
		util.DropCompiledSchema(schema)
		s.mu.Lock()
		delete(s.warmed, sha256.Sum256(schema))
		s.mu.Unlock()
	}
	fmt.Fprintf(s.stderr, "helmsweep: chart=%s rows=%d cells=%d cache-hits=%d cacheable=%s %s wall=%s\n",
		c.name, len(c.rows), len(c.rows)*len(cells), hits, cacheable, c.uncacheable, time.Since(c.start).Round(time.Millisecond))
}

func fileSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return fmt.Sprintf("%x", h.Sum(nil)), nil
}
