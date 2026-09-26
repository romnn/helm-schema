package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func testKey() cacheKey {
	return cacheKey{
		Format: cacheFormat, Executable: "e", Go: "go1.26.5", Helm: "v4.2.3", JSONSchema: "v6.0.2", Patch: "p",
		Environment: cacheEnv, Mode: "lint", KubeVersion: "1.29.0", Chart: "c", Schema: "s", Override: "o",
	}
}

const rejectLog = "==> Linting /old/w/c/cand\n[ERROR] values.yaml: - at '': false schema\n\nError: 1 chart(s) linted, 1 chart(s) failed\n"

func storedCache(t *testing.T) (verdictCache, string) {
	cache := verdictCache{t.TempDir()}
	if err := cache.store(testKey(), 1, "reject", "/old/w/c/cand", []byte(rejectLog)); err != nil {
		t.Fatal(err)
	}
	_, sum := testKey().encode()
	return cache, cache.entry(sum)
}

func TestCacheHitRewritesTheChartPath(t *testing.T) {
	cache, _ := storedCache(t)
	rc, class, log, ok := cache.lookup(testKey(), "/new/w/c/base")
	want := "==> Linting /new/w/c/base\n[ERROR] values.yaml: - at '': false schema\n\nError: 1 chart(s) linted, 1 chart(s) failed\n"
	if !ok || rc != 1 || class != "reject" || string(log) != want {
		t.Fatalf("lookup = %d %q %q %t", rc, class, log, ok)
	}
}

// Any change to a key field is a different entry.
func TestCacheKeyFieldsAreAllSignificant(t *testing.T) {
	cache, _ := storedCache(t)
	base := testKey()
	for i := range 12 {
		key := base
		fields := []*string{&key.Format, &key.Executable, &key.Go, &key.Helm, &key.JSONSchema, &key.Patch,
			&key.Environment, &key.Mode, &key.KubeVersion, &key.Chart, &key.Schema, &key.Override}
		*fields[i] += "x"
		if _, _, _, ok := cache.lookup(key, "/w"); ok {
			t.Errorf("field %d changed and still hit", i)
		}
	}
}

func rewriteEntry(t *testing.T, dir string, files map[string][]byte, fixManifest bool) {
	if fixManifest {
		all := map[string][]byte{}
		for _, name := range []string{keyFile, logFile, resultFile} {
			data, err := os.ReadFile(filepath.Join(dir, name))
			if err != nil {
				t.Fatal(err)
			}
			all[name] = data
		}
		for name, data := range files {
			all[name] = data
		}
		files[manifestFile] = manifest(all)
	}
	for name, data := range files {
		if err := os.WriteFile(filepath.Join(dir, name), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

// Damaged, incomplete or unidentified entries are misses.
func TestCacheDamageIsAMiss(t *testing.T) {
	result := func(rc int, class string) []byte {
		data, err := json.Marshal(cacheResult{RC: rc, Class: class, ChartPath: "/old/w/c/cand"})
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	damages := map[string]func(t *testing.T, dir string){
		"missing log":      func(t *testing.T, dir string) { os.Remove(filepath.Join(dir, logFile)) },
		"missing manifest": func(t *testing.T, dir string) { os.Remove(filepath.Join(dir, manifestFile)) },
		"extra file":       func(t *testing.T, dir string) { rewriteEntry(t, dir, map[string][]byte{"extra": nil}, false) },
		"log changed": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{logFile: []byte(rejectLog + " ")}, false)
		},
		"zero-byte log": func(t *testing.T, dir string) { rewriteEntry(t, dir, map[string][]byte{logFile: nil}, false) },
		"manifest changed": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{manifestFile: []byte("x")}, false)
		},
		"key changed": func(t *testing.T, dir string) { rewriteEntry(t, dir, map[string][]byte{keyFile: []byte("{}")}, true) },
		"unresolved class": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{resultFile: result(1, "unresolved:loader")}, true)
		},
		"abnormal rc": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{resultFile: result(2, "reject")}, true)
		},
		"class not the log": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{resultFile: result(1, "pass")}, true)
		},
		"log not the class": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{logFile: []byte("Error: unable to load chart\n")}, true)
		},
		"unknown result field": func(t *testing.T, dir string) {
			rewriteEntry(t, dir, map[string][]byte{resultFile: bytes.Replace(result(1, "reject"), []byte("{"), []byte(`{"x":1,`), 1)}, true)
		},
	}
	for name, damage := range damages {
		cache, dir := storedCache(t)
		if _, _, _, ok := cache.lookup(testKey(), "/w"); !ok {
			t.Fatalf("%s: the undamaged entry misses", name)
		}
		damage(t, dir)
		if _, _, _, ok := cache.lookup(testKey(), "/w"); ok {
			t.Errorf("%s: damaged entry hit", name)
		}
		// A new store replaces the damaged entry.
		if err := cache.store(testKey(), 1, "reject", "/old/w/c/cand", []byte(rejectLog)); err != nil {
			t.Fatal(err)
		}
		if _, _, _, ok := cache.lookup(testKey(), "/w"); !ok {
			t.Errorf("%s: store did not replace the damaged entry", name)
		}
	}
}

func TestCacheStoresOnlyIdentifiedVerdicts(t *testing.T) {
	cache := verdictCache{t.TempDir()}
	for _, v := range []struct {
		rc    int
		class string
	}{{1, "unresolved:loader"}, {2, "reject"}, {1, "unresolved:unknown-template-failure"}} {
		if err := cache.store(testKey(), v.rc, v.class, "/w", []byte("Error: x\n")); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := os.Stat(filepath.Join(cache.dir, "v1")); !os.IsNotExist(err) {
		t.Fatalf("an unidentified verdict was stored (%v)", err)
	}
}
