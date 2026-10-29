package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"k8s.io/apiextensions-apiserver/pkg/apis/apiextensions"
	structuralschema "k8s.io/apiextensions-apiserver/pkg/apiserver/schema"
	"k8s.io/apiextensions-apiserver/pkg/apiserver/schema/cel"
	"k8s.io/apimachinery/pkg/util/validation/field"
)

func TestInternalFailuresAreUncertain(t *testing.T) {
	path := field.NewPath("spec")
	internal := field.InternalError(path, fmt.Errorf("runtime cost information unavailable"))
	invalid := field.Invalid(path, nil, "rule failed")
	for _, errs := range []field.ErrorList{{internal}, {invalid, internal}, {internal, invalid}} {
		// An independent validation failure cannot turn an infrastructure failure into proof.
		out := withErrors("Rejected", "native validation", errs)
		if out.Verdict != "Uncertain" {
			t.Fatalf("native internal error classified as %s: %+v", out.Verdict, out)
		}
	}
}

func TestNativeCELCostLimitIsRejected(t *testing.T) {
	schema, err := structuralschema.NewStructural(&apiextensions.JSONSchemaProps{
		Type: "object",
		Properties: map[string]apiextensions.JSONSchemaProps{
			"count": {Type: "integer"},
		},
		XValidations: apiextensions.ValidationRules{{Rule: "self.count <= 5"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	validator := cel.NewValidator(schema, true, 1_000_000)
	if validator == nil {
		t.Fatal("native CEL validator missing")
	}
	// Exhaust the native runtime budget without replacing its error with a synthetic one.
	errs, _ := validator.Validate(context.Background(), field.NewPath("spec"), schema, map[string]interface{}{"count": int64(4)}, nil, 0)
	if len(errs) == 0 {
		t.Fatal("native CEL cost limit produced no error")
	}
	for _, err := range errs {
		if err.Type != field.ErrorTypeInvalid {
			t.Fatalf("native cost-limit error has unexpected type: %+v", err)
		}
	}
	out := withErrors("Rejected", "native validation", errs)
	if out.Verdict != "Rejected" {
		t.Fatalf("native cost limit classified as %s", out.Verdict)
	}
}

func TestSuppliedStatusIsUncertain(t *testing.T) {
	declaration := []byte(`{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinition","metadata":{"name":"widgets.example.org"},"spec":{"group":"example.org","names":{"kind":"Widget","plural":"widgets"},"scope":"Namespaced","versions":[{"name":"v1","served":true,"storage":true,"subresources":{"status":{}},"schema":{"openAPIV3Schema":{"type":"object","properties":{"status":{"type":"object","properties":{"count":{"type":"integer"}}}}}}}]}}`)
	for _, status := range []interface{}{nil, map[string]interface{}{}, map[string]interface{}{"count": "bad"}, map[string]interface{}{"count": int64(2)}} {
		resource, err := json.Marshal(map[string]interface{}{"apiVersion": "example.org/v1", "kind": "Widget", "metadata": map[string]interface{}{"name": "witness"}, "status": status})
		if err != nil {
			t.Fatal(err)
		}
		dir := t.TempDir()
		args := []string{}
		for i, data := range [][]byte{declaration, resource, []byte(`{"mode":"helm-4.2.3-ssa-create-strict","reference_environment":"fresh-kubernetes-1.29.0-no-admission","release_namespace":"default","created_namespaces":[]}`)} {
			path := filepath.Join(dir, fmt.Sprintf("%d.json", i))
			if err := os.WriteFile(path, data, 0600); err != nil {
				t.Fatal(err)
			}
			args = append(args, path)
		}
		// Even valid or null status is outside the supported apply request boundary.
		out, err := check(args)
		if err != nil {
			out.Verdict = "Uncertain"
		}
		if out.Verdict != "Uncertain" {
			t.Fatalf("supplied status classified as %+v", out)
		}
	}
}

func TestDeclarationsUseTheSameApplyBoundary(t *testing.T) {
	for field, value := range map[string]interface{}{"uid": "uid", "resourceVersion": "123", "managedFields": []interface{}{}, "generateName": "generated-", "ownerReferences": []interface{}{}, "finalizers": []interface{}{}, "creationTimestamp": nil} {
		raw, err := json.Marshal(map[string]interface{}{"apiVersion": "apiextensions.k8s.io/v1", "kind": "CustomResourceDefinition", "metadata": map[string]interface{}{"name": "widgets.example.org", field: value}})
		if err != nil {
			t.Fatal(err)
		}
		// A declaration must fail the shared apply boundary before schema validation.
		if _, err := validatedDeclaration(raw); err == nil || !strings.HasPrefix(err.Error(), "CRD install:") {
			t.Fatalf("declaration metadata %s: %v", field, err)
		}
	}
	raw := []byte(`{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinition","metadata":{"name":"widgets.example.org"},"status":null}`)
	if _, err := validatedDeclaration(raw); err == nil || !strings.Contains(err.Error(), "supplied status") {
		t.Fatalf("declaration status: %v", err)
	}
}

func TestSSATypedPatchRejectsObjectSetMembers(t *testing.T) {
	declaration := []byte(`{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinition","metadata":{"name":"widgets.witness.example"},"spec":{"group":"witness.example","names":{"kind":"Widget","plural":"widgets"},"scope":"Namespaced","versions":[{"name":"v1","served":true,"storage":true,"schema":{"openAPIV3Schema":{"type":"object","properties":{"spec":{"type":"object","properties":{"entries":{"type":"array","x-kubernetes-list-type":"set","items":{"type":"object","x-kubernetes-map-type":"atomic","properties":{"name":{"type":"string"}}}}}}}}}}]}}`)
	resource := []byte(`{"apiVersion":"witness.example/v1","kind":"Widget","metadata":{"name":"witness"},"spec":{"entries":[{"name":"one"}]}}`)
	context := []byte(`{"mode":"helm-4.2.3-ssa-create-strict","reference_environment":"fresh-kubernetes-1.29.0-no-admission","release_namespace":"default"}`)
	// Create validation accepts this structural schema, but SSA cannot type its object-valued set.
	out, err := validate([][]byte{declaration, resource, context})
	if err != nil || out.Verdict != "Rejected" || !strings.Contains(out.Reason, "SSA typed patch") {
		t.Fatalf("SSA object set: result=%+v err=%v", out, err)
	}
}
