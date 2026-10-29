package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/chart/loader"
	chartv2 "helm.sh/helm/v4/pkg/chart/v2"
	chartutil "helm.sh/helm/v4/pkg/chart/v2/util"
	releasev1 "helm.sh/helm/v4/pkg/release/v1"
)

func TestInstallationUsesSelectedDirectoryAndPackagedDependencies(t *testing.T) {
	isolateEnv(t)
	dir := t.TempDir()
	chart := filepath.Join(dir, "root")
	writeFiles(t, chart, map[string]string{
		"Chart.yaml":                    "apiVersion: v2\nname: root\nversion: 1.0.0\ndependencies:\n- name: child\n  alias: renamed\n  version: 1.0.0\n  condition: renamed.enabled\n",
		"values.yaml":                   "renamed:\n  enabled: true\n",
		"templates/cm.yaml":             "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: installed\n",
		"charts/child/Chart.yaml":       "apiVersion: v2\nname: child\nversion: 1.0.0\n",
		"charts/child/crds/widget.yaml": "apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\nmetadata:\n  name: widgets.example.org\n",
	})
	values := filepath.Join(dir, "values.json")
	for _, packaged := range []bool{false, true} {
		if packaged {
			loaded, err := loader.Load(filepath.Join(chart, "charts/child"))
			if err != nil {
				t.Fatal(err)
			}
			if _, err := chartutil.Save(loaded.(*chartv2.Chart), filepath.Join(chart, "charts")); err != nil {
				t.Fatal(err)
			}
			if err := os.RemoveAll(filepath.Join(chart, "charts/child")); err != nil {
				t.Fatal(err)
			}
		}
		for _, enabled := range []bool{false, true} {
			data, err := json.Marshal(map[string]interface{}{"renamed": map[string]interface{}{"enabled": enabled}})
			if err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(values, data, 0600); err != nil {
				t.Fatal(err)
			}
			evidence := &installation{}
			rc, _, stderr := templateWithEvidence("adjudication", chart, "1.29.0", []string{values}, true, evidence)
			want := 0
			if enabled {
				want = 1
			}
			if rc != 0 || len(evidence.CRDs) != want || len(evidence.Problems) != 0 {
				t.Fatalf("packaged=%v enabled=%v: rc=%d evidence=%+v stderr=%s", packaged, enabled, rc, evidence, stderr)
			}
			var obj map[string]interface{}
			if err := json.Unmarshal(evidence.Resources[0], &obj); err != nil {
				t.Fatal(err)
			}
			metadata := obj["metadata"].(map[string]interface{})
			if metadata["labels"].(map[string]interface{})["app.kubernetes.io/managed-by"] != "Helm" {
				t.Fatal("missing exact Helm ownership metadata")
			}
		}
	}
}

func TestInstallationYAMLIsStrictAndRetainsKubernetesScalars(t *testing.T) {
	documents, err := installDocuments([]byte("value: on\ninteger: 9007199254740993\nalias: &a [off]\ncopy: *a\n---\n# empty\n"))
	if err != nil {
		t.Fatal(err)
	}
	var obj map[string]interface{}
	if err := json.Unmarshal(documents[0], &obj); err != nil {
		t.Fatal(err)
	}
	if obj["value"] != true || string(documents[0]) != "{\"alias\":[false],\"copy\":[false],\"integer\":9007199254740993,\"value\":true}" {
		t.Fatalf("canonical Kubernetes YAML: %s", documents[0])
	}
	for _, source := range []string{"key: one\nkey: two\n", "good: true\n---\nbad: [\n", "- List\n- is not a CRD\n"} {
		if _, err := installDocuments([]byte(source)); err == nil {
			t.Fatalf("ambiguous/malformed stream accepted: %s", source)
		}
	}
}

func TestInstallationVetoesActiveHooksAndWholeMalformedStreams(t *testing.T) {
	for _, event := range []releasev1.HookEvent{releasev1.HookPreInstall, releasev1.HookPostInstall, releasev1.HookTest, releasev1.HookPreUpgrade} {
		evidence := &installation{}
		evidence.collect(&chartv2.Chart{}, &releasev1.Release{Name: "adjudication", Namespace: "default", Hooks: []*releasev1.Hook{{Path: "hook.yaml", Events: []releasev1.HookEvent{event}}}})
		active := event == releasev1.HookPreInstall || event == releasev1.HookPostInstall
		if (len(evidence.Problems) > 0) != active {
			t.Fatalf("hook %s: %+v", event, evidence.Problems)
		}
	}
	for _, source := range []string{
		"apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\n---\nnot: [",
		"apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\n---\napiVersion: v1\nkind: List\nitems: []\n",
	} {
		evidence := &installation{}
		evidence.collect(&chartv2.Chart{Metadata: &chartv2.Metadata{Name: "root"}, Files: []*common.File{{Name: "crds/one.yaml", Data: []byte(source)}}}, &releasev1.Release{Name: "adjudication", Namespace: "default"})
		if len(evidence.Problems) == 0 {
			t.Fatal("invalid later document failed to taint the whole installation")
		}
	}
}

func TestEmptyCRDFilesVetoTheWholeInstallation(t *testing.T) {
	for _, source := range []string{"", "# only a comment\n---\n", "---\nnull\n---\n"} {
		evidence := &installation{}
		// A valid prefix cannot hide a selected file that Helm refuses to install.
		files := []*common.File{
			{Name: "crds/valid.yaml", Data: []byte("apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\nmetadata:\n  name: widgets.example.org\n")},
			{Name: "crds/empty.yaml", Data: []byte(source)},
		}
		evidence.collect(&chartv2.Chart{Metadata: &chartv2.Metadata{Name: "root"}, Files: files}, &releasev1.Release{Name: "adjudication", Namespace: "default"})
		if len(evidence.Problems) == 0 || len(evidence.CRDs) != 2 || evidence.CRDs[1].Documents == nil || len(evidence.CRDs[1].Documents) != 0 {
			t.Fatalf("empty stream: %+v", evidence)
		}
	}
}

func TestAllAdmissionConfigurationVersionsVetoInstallation(t *testing.T) {
	for _, version := range []string{"v1", "v1beta1", "v1alpha1"} {
		evidence := &installation{}
		evidence.collect(&chartv2.Chart{Metadata: &chartv2.Metadata{Name: "root"}}, &releasev1.Release{Name: "adjudication", Namespace: "default", Manifest: "apiVersion: admissionregistration.k8s.io/" + version + "\nkind: ValidatingAdmissionPolicy\nmetadata:\n  name: policy\n"})
		if len(evidence.Problems) == 0 {
			t.Fatalf("admission %s failed to veto custom resources", version)
		}
	}
}
