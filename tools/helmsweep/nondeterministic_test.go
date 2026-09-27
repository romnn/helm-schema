package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A chart whose templates call a random function is never answered from
// the verdict cache, and none of its verdicts is stored: identical inputs
// can alternate between pass and reject.
func TestSweepNeverCachesANondeterministicChart(t *testing.T) {
	roster, rosterSHA, work := fixtureSweep(t)
	for _, cell := range cells {
		writeFiles(t, filepath.Join(work, "demo", cell), map[string]string{
			"templates/coin.yaml": "{{- if eq (randInt 0 2) 0 }}{{ fail \"coin\" }}{{ end }}\n",
		})
	}
	isolateEnv(t)
	sweepOnce(t, roster, rosterSHA, work, 2)
	removeOutputs(t, work)
	sweepOnce(t, roster, rosterSHA, work, 2)
	second := removeOutputs(t, work)
	if hits := strings.Count(string(second["cells.tsv"]), "\thit\t"); hits != 0 {
		t.Errorf("a nondeterministic chart hit the cache %d times:\n%s", hits, second["cells.tsv"])
	}
	if entries, err := os.ReadDir(filepath.Join(filepath.Dir(roster), "cache", "v1")); err == nil && len(entries) > 0 {
		t.Errorf("verdicts of a nondeterministic chart were stored: %v", entries)
	}
}
