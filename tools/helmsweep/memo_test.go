package main

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"testing"

	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/chart/common/util"
)

// memoDelta runs f and returns how many compiles and memo hits it caused.
func memoDelta(f func()) (compiles, hits int64) {
	c0, h0 := util.CompiledSchemaMemoStats()
	f()
	c1, h1 := util.CompiledSchemaMemoStats()
	return c1 - c0, h1 - h0
}

func validate(schema string, values map[string]any) string {
	if err := util.ValidateAgainstSingleSchema(common.Values(values), []byte(schema)); err != nil {
		return err.Error()
	}
	return ""
}

// schemaFor is a draft-07 schema unique to the test, not memoized when the
// test starts or after it ends.
func schemaFor(t *testing.T, body string) string {
	schema := fmt.Sprintf(`{"$schema":"http://json-schema.org/draft-07/schema#","$comment":%q,%s}`, t.Name(), body)
	util.DropCompiledSchema([]byte(schema))
	t.Cleanup(func() { util.DropCompiledSchema([]byte(schema)) })
	return schema
}

func TestMemoCompilesOnceThenHits(t *testing.T) {
	schema := schemaFor(t, `"type":"object","properties":{"replicas":{"type":"integer"}}`)
	var first, second string
	compiles, hits := memoDelta(func() {
		first = validate(schema, map[string]any{"replicas": "x"})
		second = validate(schema, map[string]any{"replicas": 2})
	})
	if compiles != 1 || hits != 1 {
		t.Fatalf("compiles=%d hits=%d, want 1 and 1", compiles, hits)
	}
	if !strings.Contains(first, "got string, want integer") || second != "" {
		t.Fatalf("verdicts %q / %q", first, second)
	}
}

func TestMemoSingleFlight(t *testing.T) {
	schema := schemaFor(t, `"type":"object","patternProperties":{"^[a-z]+$":{"type":"string","pattern":"^[a-z0-9-]{1,63}$"}}`)
	const callers = 16
	var start, done sync.WaitGroup
	start.Add(1)
	verdicts := make([]string, callers)
	compiles, hits := memoDelta(func() {
		for i := range callers {
			done.Go(func() {
				start.Wait()
				verdicts[i] = validate(schema, map[string]any{"name": fmt.Sprintf("value-%d", i)})
			})
		}
		start.Done()
		done.Wait()
	})
	if compiles != 1 || hits != callers-1 {
		t.Fatalf("compiles=%d hits=%d, want 1 and %d", compiles, hits, callers-1)
	}
	for i, v := range verdicts {
		if v != "" {
			t.Fatalf("caller %d: %q", i, v)
		}
	}
}

func TestMemoDropForgetsTheCompile(t *testing.T) {
	schema := schemaFor(t, `"type":"object"`)
	compiles, _ := memoDelta(func() {
		validate(schema, map[string]any{})
		util.DropCompiledSchema([]byte(schema))
		validate(schema, map[string]any{})
	})
	if compiles != 2 {
		t.Fatalf("compiles=%d, want 2", compiles)
	}
}

func TestMemoKeepsNoFailedCompile(t *testing.T) {
	schema := schemaFor(t, `"type":"object","properties":{"a":{"type":"string","pattern":"\\u"}}`)
	var first, second string
	compiles, hits := memoDelta(func() {
		first = validate(schema, map[string]any{})
		second = validate(schema, map[string]any{})
	})
	if compiles != 2 || hits != 0 || first == "" || first != second {
		t.Fatalf("compiles=%d hits=%d verdicts %q / %q", compiles, hits, first, second)
	}
}

// A compile that loaded an external resource is never kept: the resource may
// change while the schema bytes do not.
func TestMemoKeepsNoCompileThatLoadedAResource(t *testing.T) {
	external := filepath.Join(t.TempDir(), "external.json")
	if err := os.WriteFile(external, []byte(`{"type":"integer"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	schema := schemaFor(t, fmt.Sprintf(`"type":"object","properties":{"a":{"$ref":"file://%s"}}`, external))
	var first, second string
	compiles, hits := memoDelta(func() {
		first = validate(schema, map[string]any{"a": "x"})
		if err := os.WriteFile(external, []byte(`{"type":"string"}`), 0o644); err != nil {
			t.Fatal(err)
		}
		second = validate(schema, map[string]any{"a": "x"})
	})
	if compiles != 2 || hits != 0 || first == "" || second != "" {
		t.Fatalf("compiles=%d hits=%d verdicts %q / %q", compiles, hits, first, second)
	}
}

// Validation against one shared compiled schema from many goroutines gives
// exactly the verdicts of a fresh compile per call. Run under -race. Helm's
// message lists sibling errors in Go map order, so lines compare as a set.
func TestConcurrentValidationMatchesFreshCompiles(t *testing.T) {
	schema := schemaFor(t, `"type":"object","additionalProperties":false,
		"properties":{"name":{"type":"string","pattern":"^[a-z]+$"},"replicas":{"type":"integer","minimum":1},
		"mode":{"enum":["a","b"]},"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true},
		"nested":{"anyOf":[{"type":"null"},{"type":"object","required":["x"]}]}}`)
	instances := make([]map[string]any, 0, 64)
	for i := range 64 {
		instances = append(instances, map[string]any{
			"name": []any{"abc", "ABC", 7}[i%3], "replicas": []any{1, 0, "2", 3.5}[i%4],
			"mode": []any{"a", "c"}[i%2], "tags": []any{[]any{"x", "x"}, []any{"x"}}[i%2],
			"nested": []any{nil, map[string]any{}, map[string]any{"x": 1}}[i%3],
		})
		if i%5 == 0 {
			instances[i]["extra"] = true
		}
	}
	want := make([]string, len(instances))
	for i, instance := range instances {
		util.DropCompiledSchema([]byte(schema))
		want[i] = validate(schema, instance)
	}
	have := make([]string, len(instances))
	var wg sync.WaitGroup
	for i, instance := range instances {
		wg.Go(func() { have[i] = validate(schema, instance) })
	}
	wg.Wait()
	for i := range instances {
		if sortedLines(have[i]) != sortedLines(want[i]) {
			t.Fatalf("instance %d: shared %q, fresh %q", i, have[i], want[i])
		}
	}
}

// The staged module (stage.sh; the tests run in its build directory) is the
// tagged v4.2.3 module (h1:JEejtPE0… in go.sum), every file byte-identical
// except the one file helm-memo.patch changes (PATCH.md: upstream 7dabf3ef…,
// patched 3afeb41d…).
func TestVendoredHelmIsUpstreamPlusOnePatch(t *testing.T) {
	const (
		root      = "third_party/helm-v4.2.3"
		patched   = "pkg/chart/common/util/jsonschema.go"
		upstream  = "2b885ee5328b25b29859a02a044f2fd3774349659a5852c424e1db286fd0d5a4" // 1983 unpatched files
		patchedTo = "3afeb41df3be576b93e2cbc3f00c4d0b20bf598daca5c30583f9a6145748da0e"
		patchFile = "3c2d5c3823710929129895faecdfa7d59baea4b2bc4754d72d85e43e9039155f"
	)
	if sum, err := fileSHA256("helm-memo.patch"); err != nil || sum != patchFile {
		t.Fatalf("helm-memo.patch has sha256 %s (%v), want %s", sum, err, patchFile)
	}
	var lines []string
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		sum, err := fileSHA256(path)
		if err != nil {
			return err
		}
		if rel == patched {
			if sum != patchedTo {
				t.Errorf("%s has sha256 %s, want %s", rel, sum, patchedTo)
			}
			return nil
		}
		lines = append(lines, sum+"  "+rel+"\n")
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Slice(lines, func(i, j int) bool { return lines[i][66:] < lines[j][66:] })
	if got := fmt.Sprintf("%x", sha256.Sum256([]byte(strings.Join(lines, "")))); len(lines) != 1983 || got != upstream {
		t.Fatalf("%d unpatched files hash to %s, want 1983 hashing to %s", len(lines), got, upstream)
	}
}

func TestBuildIdentityPinsHelmJSONSchemaAndPatch(t *testing.T) {
	id, err := buildIdentity()
	if err != nil {
		t.Fatal(err)
	}
	patch, err := fileSHA256("helm-memo.patch")
	if err != nil {
		t.Fatal(err)
	}
	if id.Helm != "v4.2.3 => ./third_party/helm-v4.2.3" || id.JSONSchema != "v6.0.2" || id.Patch != patch || len(id.Executable) != 64 {
		t.Fatalf("identity %+v (patch file %s)", id, patch)
	}
}

func sortedLines(text string) string {
	lines := strings.Split(text, "\n")
	sort.Strings(lines)
	return strings.Join(lines, "\n")
}
