// Command stage copies the oracle and a digest-verified go-yaml v2.4.3 module,
// then adds an observation adapter without changing any pinned source file.
// Run from tools/goyamlsweep with go run ./stage <build directory>.
package main

import (
	"crypto/sha256"
	_ "embed"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// The content digest of the tagged module: the sha256 of `shasum -a 256` output lines
// ("<hex>  ./<path>") over every regular file, sorted bytewise by path.
const yamlPinned = "8896c202ce52cfd954fc8913e1cdd4a1c9685b6ccf9aad25272572d4cd6c0e8e"

//go:embed yaml-probe.go.txt
var yamlProbe []byte

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: go run ./stage <build directory>")
		os.Exit(2)
	}
	if err := stage(os.Args[1]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func stage(out string) error {
	out, err := filepath.Abs(out)
	if err != nil {
		return err
	}
	src, err := os.Getwd()
	if err != nil {
		return err
	}
	if err := run(src, []string{"GOFLAGS=-mod=mod"}, "go", "mod", "download", "go.yaml.in/yaml/v2@v2.4.3"); err != nil {
		return err
	}
	if err := os.RemoveAll(out); err != nil { //nolint:gosec // G703: The caller selects this disposable local build directory.
		return err
	}
	thirdParty := filepath.Join(out, "third_party")
	if err := os.MkdirAll(thirdParty, 0o755); err != nil { //nolint:gosec // G703: The caller selects this disposable local build directory.
		return err
	}
	sources, err := filepath.Glob(filepath.Join(src, "*.go"))
	if err != nil {
		return err
	}
	sources = append(sources, filepath.Join(src, "go.mod"), filepath.Join(src, "go.sum"))
	for _, file := range sources {
		if err := copyFile(file, filepath.Join(out, filepath.Base(file)), 0o644); err != nil {
			return err
		}
	}
	if err := stageYAML(src, thirdParty); err != nil {
		return err
	}
	goMod, err := os.OpenFile(filepath.Join(out, "go.mod"), os.O_APPEND|os.O_WRONLY, 0) //nolint:gosec // G703: This is the copied module in the caller-selected build directory.
	if err != nil {
		return err
	}
	if _, err := goMod.WriteString("\nreplace go.yaml.in/yaml/v2 => ./third_party/yaml-v2.4.3\n"); err != nil {
		goMod.Close()
		return err
	}
	return goMod.Close()
}

// stageYAML preserves every pinned source byte and adds an exported observation adapter.
func stageYAML(src, thirdParty string) error {
	module, err := output(src, []string{"GOFLAGS=-mod=mod"}, "go", "list", "-m", "-f", "{{.Dir}}", "go.yaml.in/yaml/v2")
	if err != nil {
		return err
	}
	destination := filepath.Join(thirdParty, "yaml-v2.4.3")
	if err := copyTree(module, destination); err != nil {
		return err
	}
	digest, err := treeDigest(destination)
	if err != nil {
		return err
	}
	if digest != yamlPinned {
		return fmt.Errorf("go-yaml v2.4.3 module copy has digest %s, not the pinned %s", digest, yamlPinned)
	}
	return os.WriteFile(filepath.Join(destination, "reader_probe.go"), yamlProbe, 0o644) //nolint:gosec // G306: Generated Go source is non-secret and must remain readable to build tools.
}

// treeDigest reproduces `find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256`.
func treeDigest(root string) (string, error) {
	var paths []string
	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error { //nolint:gosec // G703: Only the copied, pinned module tree is traversed for its digest.
		if err != nil {
			return err
		}
		if entry.Type().IsRegular() {
			rel, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			paths = append(paths, "./"+filepath.ToSlash(rel))
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	sort.Strings(paths)
	var listing strings.Builder
	for _, path := range paths {
		sum, err := fileSHA256(filepath.Join(root, filepath.FromSlash(strings.TrimPrefix(path, "./"))))
		if err != nil {
			return "", err
		}
		fmt.Fprintf(&listing, "%s  %s\n", sum, path)
	}
	total := sha256.Sum256([]byte(listing.String()))
	return hex.EncodeToString(total[:]), nil
}

func fileSHA256(path string) (string, error) {
	file, err := os.Open(path) //nolint:gosec // G703: The path is a regular source file in the module tree being digested.
	if err != nil {
		return "", err
	}
	defer file.Close()
	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}

// copyTree copies the regular files and directories under src, owner-writable (the module
// cache is read-only), keeping the executable bits.
func copyTree(src, dst string) error {
	return filepath.WalkDir(src, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if entry.IsDir() {
			return os.MkdirAll(target, 0o755) //nolint:gosec // G703: The path is under the caller-selected disposable build directory.
		}
		if !entry.Type().IsRegular() {
			return fmt.Errorf("%s: not a regular file or directory", path)
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		return copyFile(path, target, info.Mode().Perm()|0o200)
	})
}

func copyFile(src, dst string, mode fs.FileMode) error {
	in, err := os.Open(src) //nolint:gosec // G703: This copies a selected oracle source or verified module file.
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, mode) //nolint:gosec // G703: The destination is under the caller-selected disposable build directory.
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}

func run(dir string, env []string, name string, args ...string) error {
	cmd := exec.Command(name, args...) //nolint:gosec // G702: Callers select fixed Go and Git commands with local build arguments.
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), env...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("%s %s: %w", name, strings.Join(args, " "), err)
	}
	return nil
}

func output(dir string, env []string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), env...)
	cmd.Stderr = os.Stderr
	text, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%s %s: %w", name, strings.Join(args, " "), err)
	}
	return strings.TrimSpace(string(text)), nil
}
