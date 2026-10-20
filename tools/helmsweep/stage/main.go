// Command stage stages a build-local copy of the helmsweep module with Helm v4.2.3 patched by
// helm-memo.patch (PATCH.md): it downloads the tagged module (Go verifies it against go.sum),
// copies it writable to <out>/third_party/helm-v4.2.3, refuses unless its content digest is the
// pinned one, applies the patch strictly (git apply --check, then git apply: no fuzz, no
// rejects), writes a go.mod that replaces helm.sh/helm/v4 with the copy, and records the
// patch's sha256 in <out>/helm-memo.patch.sha256. The source tree is never written.
//
// Run it from tools/helmsweep: go run ./stage <build directory>. It uses only the standard
// library, so it builds with the pinned Go toolchain on every platform, Windows included.
package main

import (
	"crypto/sha256"
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
const pinned = "ad77014f810d103768b7dc7aa3999b076249a315c4b5e436bc1531439867305f"

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
	modMod := []string{"GOFLAGS=-mod=mod"}
	if err := run(src, modMod, "go", "mod", "download", "helm.sh/helm/v4@v4.2.3"); err != nil {
		return err
	}
	module, err := output(src, modMod, "go", "list", "-m", "-f", "{{.Dir}}", "helm.sh/helm/v4")
	if err != nil {
		return err
	}
	if err := os.RemoveAll(out); err != nil {
		return err
	}
	thirdParty := filepath.Join(out, "third_party")
	if err := os.MkdirAll(thirdParty, 0o755); err != nil {
		return err
	}
	sources, err := filepath.Glob(filepath.Join(src, "*.go"))
	if err != nil {
		return err
	}
	sources = append(sources, filepath.Join(src, "go.mod"), filepath.Join(src, "go.sum"), filepath.Join(src, "helm-memo.patch"))
	for _, file := range sources {
		if err := copyFile(file, filepath.Join(out, filepath.Base(file)), 0o644); err != nil {
			return err
		}
	}
	if err := copyTree(filepath.Join(src, "testdata"), filepath.Join(out, "testdata")); err != nil {
		return err
	}
	helm := filepath.Join(thirdParty, "helm-v4.2.3")
	if err := copyTree(module, helm); err != nil {
		return err
	}
	digest, err := treeDigest(helm)
	if err != nil {
		return err
	}
	if digest != pinned {
		return fmt.Errorf("helm v4.2.3 module copy has digest %s, not the pinned %s", digest, pinned)
	}
	patch := filepath.Join(out, "helm-memo.patch")
	ceiling := []string{"GIT_CEILING_DIRECTORIES=" + thirdParty}
	if err := run(helm, ceiling, "git", "apply", "--check", patch); err != nil {
		return err
	}
	if err := run(helm, ceiling, "git", "apply", patch); err != nil {
		return err
	}
	goMod, err := os.OpenFile(filepath.Join(out, "go.mod"), os.O_APPEND|os.O_WRONLY, 0)
	if err != nil {
		return err
	}
	if _, err := goMod.WriteString("\nreplace helm.sh/helm/v4 => ./third_party/helm-v4.2.3\n"); err != nil {
		goMod.Close()
		return err
	}
	if err := goMod.Close(); err != nil {
		return err
	}
	patchSum, err := fileSHA256(patch)
	if err != nil {
		return err
	}
	return os.WriteFile(patch+".sha256", []byte(patchSum+"\n"), 0o644)
}

// treeDigest reproduces `find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256`.
func treeDigest(root string) (string, error) {
	var paths []string
	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
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
	file, err := os.Open(path)
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
			return os.MkdirAll(target, 0o755)
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
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, mode)
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
