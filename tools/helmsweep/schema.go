package main

import (
	"bytes"
	"crypto/sha256"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"helm.sh/helm/v4/pkg/chart/v2/loader"
	"helm.sh/helm/v4/pkg/ignore"
)

// schemaIgnore blanks schema-excluding rules and preserves other lines' bytes and parsing context.
// Evaluate rules separately because Helm returns on the first excluding rule.
func schemaIgnore(data []byte, info os.FileInfo) ([]byte, error) {
	if _, err := ignore.Parse(bytes.NewReader(data)); err != nil {
		return nil, err
	}
	var kept bytes.Buffer
	bom := []byte{0xef, 0xbb, 0xbf}
	for index, line := range bytes.SplitAfter(data, []byte("\n")) {
		input := line
		if index > 0 {
			input = append([]byte("\n"), line...)
		}
		rules, err := ignore.Parse(bytes.NewReader(input))
		if err != nil {
			return nil, err
		}
		if rules.Ignore(schemaFile, info) {
			// Preserve the file BOM and line endings so later BOMs stay literal.
			if index == 0 && bytes.HasPrefix(line, bom) {
				kept.Write(bom)
			}
			if bytes.HasSuffix(line, []byte("\r\n")) {
				kept.WriteString("\r\n")
			} else if bytes.HasSuffix(line, []byte("\n")) {
				kept.WriteByte('\n')
			}
			continue
		}
		kept.Write(line)
	}
	return kept.Bytes(), nil
}

func prepareSchemaIgnore(root string) error {
	info, err := os.Stat(filepath.Join(root, schemaFile))
	if err != nil {
		return err
	}
	path := filepath.Join(root, ignore.HelmIgnore)
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	prepared, err := schemaIgnore(data, info)
	if err != nil || bytes.Equal(data, prepared) {
		return err
	}
	return os.WriteFile(path, prepared, 0o644)
}

func loadedSchemaSHA(root string) (string, error) {
	chart, err := loader.LoadDir(root)
	if err != nil {
		return "", err
	}
	if len(chart.Schema) == 0 {
		return "", errors.New("Helm loaded no root values.schema.json")
	}
	return fmt.Sprintf("%x", sha256.Sum256(chart.Schema)), nil
}

func cmdSchema(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("schema", flag.ContinueOnError)
	flags.SetOutput(stderr)
	prepare := flags.Bool("prepare", false, "blank only ignore rules that exclude the root schema")
	if err := flags.Parse(args); err != nil || flags.NArg() != 1 {
		fmt.Fprintln(stderr, "usage: helmsweep schema [--prepare] CHART")
		return exitRefused
	}
	root := flags.Arg(0)
	if *prepare {
		if err := prepareSchemaIgnore(root); err != nil {
			fmt.Fprintln(stderr, "helmsweep: schema:", err)
			return exitRefused
		}
	}
	digest, err := loadedSchemaSHA(root)
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep: schema:", err)
		return exitRefused
	}
	if _, err := fmt.Fprintln(stdout, digest); err != nil {
		return exitHarness
	}
	return 0
}

// schemaCopyDigest permits exactly the ignore edit derived from the plain copy.
// Hash the original ignore bytes so the shared cache digest remains based on plain.
func schemaCopyDigest(root, plain string) (string, []byte, error) {
	data, err := os.ReadFile(filepath.Join(plain, ignore.HelmIgnore))
	if errors.Is(err, os.ErrNotExist) {
		return copyDigest(root)
	}
	if err != nil {
		return "", nil, err
	}
	info, err := os.Stat(filepath.Join(root, schemaFile))
	if err != nil {
		return "", nil, err
	}
	want, err := schemaIgnore(data, info)
	if err != nil {
		return "", nil, err
	}
	got, err := os.ReadFile(filepath.Join(root, ignore.HelmIgnore))
	if err != nil || !bytes.Equal(got, want) {
		return "", nil, fmt.Errorf("%s .helmignore differs from schema preparation (%v)", root, err)
	}
	return copyDigestWithIgnore(root, data)
}
