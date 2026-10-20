package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"helm.sh/helm/v4/pkg/chart/v2/loader"
)

func TestSchemaPreparationPreservesLiteralBOMContext(t *testing.T) {
	root := t.TempDir()
	writeFiles(t, root, fixtureChart)
	// The second-line BOM is part of a nonmatching pattern, so this template must remain loaded.
	writeFiles(t, root, map[string]string{
		schemaFile:            fixtureCand,
		".helmignore":         "values.schema.json\n\xef\xbb\xbftemplates/keep.yaml\n",
		"templates/keep.yaml": "# Retained template\n",
	})
	before, err := loader.LoadDir(root)
	if err != nil {
		t.Fatal(err)
	}
	var out, log bytes.Buffer
	if rc := cmdSchema([]string{"--prepare", root}, &out, &log); rc != 0 || out.String() != sha(fixtureCand)+"\n" {
		t.Fatalf("schema preparation: rc=%d out=%s log=%s", rc, &out, &log)
	}
	after, err := loader.LoadDir(root)
	if err != nil {
		t.Fatal(err)
	}
	if len(before.Templates) != len(after.Templates) {
		t.Errorf("unrelated template visibility changed: before=%d after=%d", len(before.Templates), len(after.Templates))
	}
	for _, expected := range before.Templates {
		found := false
		for _, actual := range after.Templates {
			if actual.Name == expected.Name && bytes.Equal(actual.Data, expected.Data) {
				found = true
			}
		}
		if !found {
			t.Errorf("unrelated template hidden or changed: %s", expected.Name)
		}
	}
	data, err := os.ReadFile(filepath.Join(root, ".helmignore"))
	if err != nil || string(data) != "\n\xef\xbb\xbftemplates/keep.yaml\n" {
		t.Errorf("line context changed: ignore=%q err=%v", data, err)
	}
}

func TestSchemaIgnoreUsesHelmRulesAndPreservesOtherBytes(t *testing.T) {
	root := t.TempDir()
	writeFiles(t, root, map[string]string{schemaFile: fixtureCand})
	info, err := os.Stat(filepath.Join(root, schemaFile))
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct{ name, input, want string }{
		{"literal", "# keep\r\nvalues.schema.json\r\nnotes.txt\r\n", "# keep\r\n\r\nnotes.txt\r\n"},
		{"anchored", "/values.schema.json\nnotes.txt\n", "\nnotes.txt\n"},
		{"glob", "*.json\nnotes.txt\n", "\nnotes.txt\n"},
		{"class", "values.[s]chema.json\nnotes.txt\n", "\nnotes.txt\n"},
		{"negation excludes nonmatches", "!notes.txt\nnotes.txt\n", "\nnotes.txt\n"},
		{"negation matches", "!values.schema.json\nnotes.txt\n", "!values.schema.json\nnotes.txt\n"},
		{"directory", "values.schema.json/\nnotes.txt\n", "values.schema.json/\nnotes.txt\n"},
		{"multiple", "values.schema.json\n*.json\nnotes.txt\n", "\n\nnotes.txt\n"},
		{"unterminated", "notes.txt\n values.schema.json ", "notes.txt\n"},
		{"BOM", "\xef\xbb\xbfvalues.schema.json\nnotes.txt\n", "\xef\xbb\xbf\nnotes.txt\n"},
		{"later BOM is literal", "# keep\n\xef\xbb\xbfvalues.schema.json\n", "# keep\n\xef\xbb\xbfvalues.schema.json\n"},
		{"control", "# keep\r\nnotes.txt", "# keep\r\nnotes.txt"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := schemaIgnore([]byte(tc.input), info)
			if err != nil || string(got) != tc.want {
				t.Fatalf("schemaIgnore = %q, %v; want %q", got, err, tc.want)
			}
		})
	}
	for _, invalid := range []string{"**.json\n", "[\n"} {
		if _, err := schemaIgnore([]byte(invalid), info); err == nil {
			t.Fatalf("accepted invalid Helm rule %q", invalid)
		}
	}
}

func TestSchemaCommandProvesLoadedBytes(t *testing.T) {
	root := t.TempDir()
	writeFiles(t, root, fixtureChart)
	writeFiles(t, root, map[string]string{schemaFile: fixtureCand, ".helmignore": "values.schema.json\nnotes.txt\n"})
	var out, log bytes.Buffer
	if rc := cmdSchema([]string{root}, &out, &log); rc != exitRefused || out.Len() != 0 {
		t.Fatalf("hidden schema accepted: rc=%d out=%s log=%s", rc, &out, &log)
	}
	if rc := cmdSchema([]string{"--prepare", root}, &out, &log); rc != 0 || out.String() != sha(fixtureCand)+"\n" {
		t.Fatalf("schema preparation: rc=%d out=%s log=%s", rc, &out, &log)
	}
	data, _ := os.ReadFile(filepath.Join(root, ".helmignore"))
	if string(data) != "\nnotes.txt\n" {
		t.Fatalf("unrelated ignore bytes changed: %q", data)
	}
	// Helm strips a schema BOM; the report must describe loaded bytes, not disk bytes.
	writeFiles(t, root, map[string]string{schemaFile: "\xef\xbb\xbf" + fixtureCand})
	out.Reset()
	if rc := cmdSchema([]string{root}, &out, &log); rc != 0 || out.String() != sha(fixtureCand)+"\n" {
		t.Fatalf("not reporting loaded bytes: rc=%d out=%s log=%s", rc, &out, &log)
	}
	writeFiles(t, root, map[string]string{".helmignore": "Chart.yaml\n"})
	out.Reset()
	if rc := cmdSchema([]string{root}, &out, &log); rc != exitRefused {
		t.Fatalf("accepted chart with hidden manifest: %d", rc)
	}
}

func TestSweepRequiresLoadedSchemaAndExactIgnorePreparation(t *testing.T) {
	for _, mutation := range []string{"prepared", "hidden", "unrelated edit", "schema BOM"} {
		t.Run(mutation, func(t *testing.T) {
			roster, rosterSHA, work := fixtureSweep(t)
			for _, cell := range cells {
				writeFiles(t, filepath.Join(work, "demo", cell), map[string]string{".helmignore": "values.schema.json\nnotes.txt\n"})
			}
			for _, cell := range []string{"base", "cand"} {
				if mutation != "hidden" {
					if err := prepareSchemaIgnore(filepath.Join(work, "demo", cell)); err != nil {
						t.Fatal(err)
					}
				}
			}
			if mutation == "unrelated edit" {
				writeFiles(t, filepath.Join(work, "demo", "cand"), map[string]string{".helmignore": ""})
			}
			if mutation == "schema BOM" {
				withBOM := "\xef\xbb\xbf" + fixtureCand
				writeFiles(t, filepath.Join(work, "demo", "cand"), map[string]string{schemaFile: withBOM})
				writeFiles(t, filepath.Join(filepath.Dir(roster), "plan", "demo"), map[string]string{"cand.schema.json": withBOM})
				rosterSHA = rewriteRoster(t, roster, func(text string) string {
					return strings.ReplaceAll(text, sha(fixtureCand), sha(withBOM))
				})
			}
			s := newSweeper(t, roster, work)
			_, err := s.prepare(roster, rosterSHA)
			if (mutation == "prepared") != (err == nil) {
				t.Fatalf("%s: prepare error = %v", mutation, err)
			}
			plain, _ := os.ReadFile(filepath.Join(work, "demo", "plain", ".helmignore"))
			if !bytes.Equal(plain, []byte("values.schema.json\nnotes.txt\n")) {
				t.Fatalf("plain ignore rules changed: %q", plain)
			}
		})
	}
}
