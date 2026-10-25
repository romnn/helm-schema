package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"

	"helm.sh/helm/v4/pkg/action"
	"helm.sh/helm/v4/pkg/chart"
	chartv2 "helm.sh/helm/v4/pkg/chart/v2"
	releasev1 "helm.sh/helm/v4/pkg/release/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	kyaml "k8s.io/apimachinery/pkg/util/yaml"
	"k8s.io/cli-runtime/pkg/resource"
	"sigs.k8s.io/yaml"
)

// installation is evidence from the same dependency-processed chart Helm rendered.
// Problems taint the whole custom-resource installation, including valid prefixes.
type installation struct {
	KubernetesVersion string            `json:"kubernetes_version"`
	Protocol          string            `json:"protocol"`
	Namespace         string            `json:"namespace"`
	Release           string            `json:"release"`
	CRDs              []crdFile         `json:"crds"`
	Resources         []json.RawMessage `json:"resources"`
	Hooks             []*releasev1.Hook `json:"hooks"`
	Problems          []string          `json:"problems"`
}

type crdFile struct {
	Filename  string            `json:"filename"`
	Raw       string            `json:"raw"`
	Documents []json.RawMessage `json:"documents"`
}

func (e *installation) collect(ch chart.Charter, rel *releasev1.Release) {
	if e == nil {
		return
	}
	e.Protocol = "helm-schema/installation/v1"
	e.CRDs = []crdFile{}
	e.Resources = []json.RawMessage{}
	e.Problems = []string{}
	if rel == nil {
		e.Problems = append(e.Problems, "missing release")
		return
	}
	e.Namespace = rel.Namespace
	e.Release = rel.Name
	e.Hooks = rel.Hooks
	selected, ok := ch.(*chartv2.Chart)
	if !ok {
		e.Problems = append(e.Problems, "unsupported chart type")
		return
	}
	for _, file := range selected.CRDObjects() {
		documents, err := installDocuments(file.File.Data)
		if documents == nil {
			documents = []json.RawMessage{}
		}
		e.CRDs = append(e.CRDs, crdFile{Filename: file.Filename, Raw: string(file.File.Data), Documents: documents})
		if len(documents) == 0 {
			e.Problems = append(e.Problems, fmt.Sprintf("CRD %s: resources are empty", file.Filename))
		}
		if err != nil {
			e.Problems = append(e.Problems, fmt.Sprintf("CRD %s: %v", file.Filename, err))
		}
		for _, doc := range documents {
			var obj unstructured.Unstructured
			if err := obj.UnmarshalJSON(doc); err != nil || obj.GetAPIVersion() != "apiextensions.k8s.io/v1" || obj.GetKind() != "CustomResourceDefinition" {
				e.Problems = append(e.Problems, "CRD file contains unsupported or non-CRD document")
			}
		}
	}
	documents, err := installDocuments([]byte(rel.Manifest))
	if err != nil {
		e.Problems = append(e.Problems, err.Error())
		return
	}
	for _, doc := range documents {
		var obj unstructured.Unstructured
		if err := obj.UnmarshalJSON(doc); err != nil {
			e.Problems = append(e.Problems, err.Error())
			continue
		}
		if obj.GetKind() == "List" {
			e.Problems = append(e.Problems, "List installation expansion is unproven")
		}
		if obj.GetKind() == "CustomResourceDefinition" || obj.GroupVersionKind().Group == "admissionregistration.k8s.io" {
			e.Problems = append(e.Problems, "render changes CRDs or admission")
		}
		// Use Helm's visitor, including overwrite semantics and metadata type errors.
		if err := action.ReleaseMetadataVisitor(rel.Name, rel.Namespace)(&resource.Info{Object: &obj}, nil); err != nil {
			e.Problems = append(e.Problems, err.Error())
			continue
		}
		raw, err := obj.MarshalJSON()
		if err != nil {
			e.Problems = append(e.Problems, err.Error())
			continue
		}
		e.Resources = append(e.Resources, raw)
	}
	for _, hook := range rel.Hooks {
		for _, event := range hook.Events {
			if event == releasev1.HookPreInstall || event == releasev1.HookPostInstall {
				e.Problems = append(e.Problems, "active install hook: "+hook.Path)
			}
		}
	}
}

// Strict YAML conversion rejects duplicate keys instead of choosing an interpretation.
// Kubernetes' YAML conversion retains YAML 1.1 scalars and JSON integer bytes.
func installDocuments(source []byte) ([]json.RawMessage, error) {
	reader := kyaml.NewYAMLReader(bufio.NewReader(bytes.NewReader(source)))
	docs := []json.RawMessage{}
	for {
		raw, err := reader.Read()
		if errors.Is(err, io.EOF) {
			return docs, nil
		}
		if err != nil {
			return nil, err
		}
		doc, err := yaml.YAMLToJSONStrict(raw)
		if err != nil {
			return nil, err
		}
		if bytes.Equal(bytes.TrimSpace(doc), []byte("null")) {
			continue
		}
		var obj map[string]json.RawMessage
		if err := json.Unmarshal(doc, &obj); err != nil || obj == nil {
			return nil, fmt.Errorf("installation document must be an object")
		}
		docs = append(docs, doc)
	}
}
