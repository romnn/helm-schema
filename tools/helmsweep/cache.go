package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

// cacheKey is everything a cell's verdict depends on. The copy's path is not
// part of it: a stored log names the path it ran at and a hit rewrites that
// path to the current one before it is reclassified.
type cacheKey struct {
	Format      string `json:"format"`
	Executable  string `json:"executable_sha256"`
	Go          string `json:"go"`
	Helm        string `json:"helm"`
	HelmBuild   string `json:"helm_build"`
	JSONSchema  string `json:"jsonschema"`
	Patch       string `json:"patch_sha256"`
	Environment string `json:"environment"`
	Mode        string `json:"mode"`
	KubeVersion string `json:"kube_version"`
	Chart       string `json:"chart_copy_digest"`
	Schema      string `json:"schema_sha256"`
	Override    string `json:"override_sha256"`
}

type cacheResult struct {
	RC        int    `json:"rc"`
	Class     string `json:"class"`
	ChartPath string `json:"chart_path"`
}

const (
	cacheFormat   = "helmsweep-cache/2"
	cacheEnv      = "re-executed before initialization: HOME=<empty>, PATH=/usr/bin:/bin"
	keyFile       = "key.json"
	resultFile    = "result.json"
	logFile       = "log"
	manifestFile  = "MANIFEST"
	cacheTempRoot = "tmp"
)

// verdictCache stores identified verdicts (pass or reject, rc 0 or 1) under
// dir/v1/<key sha[:2]>/<key sha>/. It is a speed optimisation only: anything
// that does not verify exactly is a miss.
type verdictCache struct{ dir string }

func (k cacheKey) encode() ([]byte, string) {
	data, err := json.Marshal(k)
	if err != nil {
		panic(err) // a struct of strings always marshals
	}
	return data, fmt.Sprintf("%x", sha256.Sum256(data))
}

func (c verdictCache) entry(sum string) string {
	return filepath.Join(c.dir, "v1", sum[:2], sum)
}

func manifest(files map[string][]byte) []byte {
	var out bytes.Buffer
	for _, name := range []string{keyFile, logFile, resultFile} {
		fmt.Fprintf(&out, "%x  %s\n", sha256.Sum256(files[name]), name)
	}
	return out.Bytes()
}

// lookup returns the stored rc, class and log (rewritten to chartPath) of
// key. Every file must be present and listed in the manifest with its hash,
// the stored key must equal key, the verdict must be identified, and the log
// must reclassify to the stored class before and after the rewrite.
func (c verdictCache) lookup(key cacheKey, chartPath string) (int, string, []byte, bool) {
	keyBytes, sum := key.encode()
	dir := c.entry(sum)
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0, "", nil, false
	}
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		names = append(names, e.Name())
	}
	if !slices.Equal(names, []string{manifestFile, keyFile, logFile, resultFile}) {
		return 0, "", nil, false
	}
	files := map[string][]byte{}
	for _, name := range names {
		if files[name], err = os.ReadFile(filepath.Join(dir, name)); err != nil {
			return 0, "", nil, false
		}
	}
	if !bytes.Equal(files[manifestFile], manifest(files)) || !bytes.Equal(files[keyFile], keyBytes) {
		return 0, "", nil, false
	}
	var result cacheResult
	decoder := json.NewDecoder(bytes.NewReader(files[resultFile]))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&result) != nil || (result.RC != 0 && result.RC != 1) || !identified(result.Class) || result.ChartPath == "" {
		return 0, "", nil, false
	}
	log := files[logFile]
	if classify(key.Mode, result.RC, log) != result.Class {
		return 0, "", nil, false
	}
	log = []byte(strings.ReplaceAll(string(log), result.ChartPath, chartPath))
	if classify(key.Mode, result.RC, log) != result.Class {
		return 0, "", nil, false
	}
	return result.RC, result.Class, log, true
}

// store keeps an identified verdict: written to a temporary directory, then
// renamed into place, replacing a damaged entry.
func (c verdictCache) store(key cacheKey, rc int, class, chartPath string, log []byte) error {
	if (rc != 0 && rc != 1) || !identified(class) {
		return nil
	}
	keyBytes, sum := key.encode()
	result, err := json.Marshal(cacheResult{RC: rc, Class: class, ChartPath: chartPath})
	if err != nil {
		return err
	}
	files := map[string][]byte{keyFile: keyBytes, resultFile: result, logFile: log}
	files[manifestFile] = manifest(files)

	if err := os.MkdirAll(filepath.Join(c.dir, "v1", cacheTempRoot), 0o755); err != nil {
		return err
	}
	tmp, err := os.MkdirTemp(filepath.Join(c.dir, "v1", cacheTempRoot), sum)
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)
	for name, data := range files {
		if err := os.WriteFile(filepath.Join(tmp, name), data, 0o644); err != nil {
			return err
		}
	}
	final := c.entry(sum)
	if err := os.MkdirAll(filepath.Dir(final), 0o755); err != nil {
		return err
	}
	if err := os.RemoveAll(final); err != nil {
		return err
	}
	return os.Rename(tmp, final)
}
