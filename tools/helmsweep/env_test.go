package main

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"helm.sh/helm/v4/pkg/cli"
)

// buildBinary builds this command into a temporary directory.
func buildBinary(t *testing.T) string {
	binary := filepath.Join(t.TempDir(), "helmsweep")
	build := exec.Command("go", "build", "-o", binary, ".")
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("go build: %v\n%s", err, out)
	}
	return binary
}

// With --helm-env-clear the process's own environment never reaches Helm,
// including what client-go reads while packages initialize: a kubeconfig in
// the original HOME naming a namespace does not change the release
// namespace (the CLI under the cleared environment renders in "default").
func TestHelmEnvClearPrecedesPackageInitialization(t *testing.T) {
	binary := buildBinary(t)
	dir := t.TempDir()
	writeFiles(t, dir, map[string]string{
		"home/.kube/config": "apiVersion: v1\nkind: Config\ncurrent-context: leaked\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:1\n" +
			"contexts:\n- name: leaked\n  context:\n    cluster: c\n    namespace: leaked\n    user: u\nusers:\n- name: u\n  user: {}\n",
		"chart/Chart.yaml": "apiVersion: v2\nname: demo\nversion: 0.1.0\n",
		"chart/templates/cm.yaml": "{{- if ne .Release.Namespace \"default\" }}{{ fail (printf \"namespace %s\" .Release.Namespace) }}{{ end }}\n" +
			"apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: x\n",
		"values.json": "{}",
	})
	for _, mode := range []string{"template", "lint"} {
		cmd := exec.Command(binary, mode, "--helm-env-clear", "--kube-version", "1.29.0",
			"--values", filepath.Join(dir, "values.json"), filepath.Join(dir, "chart"))
		cmd.Env = []string{"HOME=" + filepath.Join(dir, "home"), "PATH=" + os.Getenv("PATH")}
		out, err := cmd.CombinedOutput()
		if err != nil || strings.Contains(string(out), "leaked") {
			t.Errorf("%s: %v\n%s", mode, err, out)
		}
	}
}

// Configuration whose CLI path helmsweep does not reproduce is refused:
// debug output, a storage driver, a registry or repository configuration,
// installed plugins; the empty configuration is not.
func TestUnreproducedConfigurationIsRefused(t *testing.T) {
	for name, setup := range map[string]func(home string){
		"debug":      func(string) { t.Setenv("HELM_DEBUG", "true") },
		"driver":     func(string) { t.Setenv("HELM_DRIVER", "memory") },
		"registry":   func(home string) { writeFiles(t, home, map[string]string{"config/registry/config.json": "{}"}) },
		"repository": func(home string) { writeFiles(t, home, map[string]string{"config/repositories.yaml": "{}"}) },
		"plugins":    func(home string) { writeFiles(t, home, map[string]string{"data/plugins/p/plugin.yaml": "name: p\n"}) },
	} {
		t.Run(name, func(t *testing.T) {
			isolateEnv(t)
			home := t.TempDir()
			t.Setenv("HELM_CONFIG_HOME", filepath.Join(home, "config"))
			t.Setenv("HELM_DATA_HOME", filepath.Join(home, "data"))
			t.Setenv("HELM_CACHE_HOME", filepath.Join(home, "cache"))
			if err := unsupportedConfiguration(cli.New()); err != nil {
				t.Fatalf("the empty configuration is refused: %v", err)
			}
			setup(home)
			if unsupportedConfiguration(cli.New()) == nil {
				t.Errorf("%s is not refused", name)
			}
			var out, errs bytes.Buffer
			if rc := cmdServe(nil, strings.NewReader(""), &out, &errs); rc != exitRefused {
				t.Errorf("serve under %s exited %d: %s", name, rc, errs.String())
			}
		})
	}
}
