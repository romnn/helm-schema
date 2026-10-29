// crdcheck validates canonical JSON resources with pinned Kubernetes code for maintainer adjudication.
// Installation scope and declaration conflicts must be established by its caller.
package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sort"

	"k8s.io/apiextensions-apiserver/pkg/apis/apiextensions"
	apiextensionsv1 "k8s.io/apiextensions-apiserver/pkg/apis/apiextensions/v1"
	crdvalidation "k8s.io/apiextensions-apiserver/pkg/apis/apiextensions/validation"
	structuralschema "k8s.io/apiextensions-apiserver/pkg/apiserver/schema"
	"k8s.io/apiextensions-apiserver/pkg/apiserver/schema/defaulting"
	"k8s.io/apiextensions-apiserver/pkg/apiserver/schema/objectmeta"
	"k8s.io/apiextensions-apiserver/pkg/apiserver/schema/pruning"
	"k8s.io/apiextensions-apiserver/pkg/apiserver/validation"
	"k8s.io/apiextensions-apiserver/pkg/controller/openapi/builder"
	"k8s.io/apiextensions-apiserver/pkg/registry/customresource"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/util/managedfields"
	"k8s.io/apimachinery/pkg/util/validation/field"
)

const protocol = "helm-schema/crdcheck/v3"

// EffectiveJSON is output evidence, never a cache lookup input.
type response struct {
	Protocol      string `json:"protocol"`
	RequestSHA256 string `json:"request_sha256"`
	result
}

type result struct {
	EffectiveJSON   string       `json:"effective_json,omitempty"`
	EffectiveSHA256 string       `json:"effective_sha256,omitempty"`
	Verdict         string       `json:"verdict"`
	Reason          string       `json:"reason,omitempty"`
	Errors          []fieldError `json:"errors,omitempty"`
}

type fieldError struct {
	Path   string `json:"path"`
	Type   string `json:"type"`
	Detail string `json:"detail"`
}

type requestContext struct {
	Declarations         []json.RawMessage `json:"declarations"`
	Mode                 string            `json:"mode"`
	ReferenceEnvironment string            `json:"reference_environment"`
	ReleaseNamespace     string            `json:"release_namespace"`
	CreatedNamespaces    []string          `json:"created_namespaces"`
	Identity             string            `json:"identity"`
}

func main() {
	out, err := check(os.Args[1:])
	if err != nil {
		out.result = result{Verdict: "Uncertain", Reason: err.Error()}
	}
	if err := json.NewEncoder(os.Stdout).Encode(out); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func check(args []string) (response, error) {
	out := response{Protocol: protocol}
	if len(args) != 3 {
		return out, fmt.Errorf("usage: crdcheck DECLARATION.json RESOURCE.json CONTEXT.json")
	}
	inputs := make([][]byte, 3)
	hash := sha256.New()
	for i, path := range args {
		raw, err := os.ReadFile(path)
		if err != nil {
			return out, err
		}
		inputs[i] = raw
		if err := binary.Write(hash, binary.BigEndian, uint64(len(raw))); err != nil {
			return out, err
		}
		hash.Write(raw)
	}
	out.RequestSHA256 = fmt.Sprintf("%x", hash.Sum(nil))
	checked, err := validate(inputs)
	out.result = checked
	return out, err
}

func validate(inputs [][]byte) (result, error) {
	var request requestContext
	if err := readJSON(inputs[2], &request); err != nil {
		return result{}, err
	}
	if request.Mode != "helm-4.2.3-ssa-create-strict" || request.ReferenceEnvironment != "fresh-kubernetes-1.29.0-no-admission" || request.ReleaseNamespace == "" {
		return result{}, fmt.Errorf("unsupported or unproven request/reference environment")
	}
	crd, err := validatedDeclaration(inputs[0])
	if err != nil {
		return result{}, err
	}
	for _, raw := range request.Declarations {
		if _, err := validatedDeclaration(raw); err != nil {
			return result{}, err
		}
	}
	ctx := context.Background()
	resource, err := applyObject(inputs[1])
	if err != nil {
		return result{}, err
	}
	gvk := resource.GroupVersionKind()
	if gvk.Group != crd.Spec.Group || gvk.Kind != crd.Spec.Names.Kind {
		return result{}, fmt.Errorf("resource GVK does not match declaration")
	}
	if crd.Spec.Scope == apiextensions.NamespaceScoped {
		namespace, _, err := unstructured.NestedString(resource.Object, "metadata", "namespace")
		if err != nil {
			return result{}, err
		}
		if namespace == "" {
			namespace = request.ReleaseNamespace
			resource.SetNamespace(namespace)
		}
		exists := namespace == request.ReleaseNamespace
		for _, created := range request.CreatedNamespaces {
			exists = exists || namespace == created
		}
		if !exists {
			return result{}, fmt.Errorf("namespace %q is neither the release namespace nor created by the render", namespace)
		}
	}
	served := false
	for _, version := range crd.Spec.Versions {
		if version.Name == gvk.Version {
			served = version.Served
		}
	}
	if !served {
		return result{}, fmt.Errorf("resource version %q is not served", gvk.Version)
	}
	selected, err := apiextensions.GetSchemaForVersion(crd, gvk.Version)
	if err != nil {
		return result{}, err
	}
	if selected == nil || selected.OpenAPIV3Schema == nil {
		return result{}, fmt.Errorf("served version has no structural schema")
	}
	structural, err := structuralschema.NewStructural(selected.OpenAPIV3Schema)
	if err != nil {
		return result{}, err
	}
	if errs := structuralschema.ValidateStructural(field.NewPath("schema"), structural); len(errs) != 0 {
		return withErrors("Uncertain", "nonstructural declaration", errs), nil
	}
	schemaValidator, _, err := validation.NewSchemaValidator(selected.OpenAPIV3Schema)
	if err != nil {
		return result{}, err
	}
	subresources, err := apiextensions.GetSubresourcesForVersion(crd, gvk.Version)
	if err != nil {
		return result{}, err
	}
	if subresources == nil {
		subresources = &apiextensions.CustomResourceSubresources{}
	}

	// SSA validates the original patch before pruning, defaults or the create strategy.
	var external apiextensionsv1.CustomResourceDefinition
	if err := apiextensionsv1.Convert_apiextensions_CustomResourceDefinition_To_v1_CustomResourceDefinition(crd, &external, nil); err != nil {
		return result{}, err
	}
	models, err := builder.BuildOpenAPIV3(&external, gvk.Version, builder.Options{})
	if err != nil {
		return result{}, err
	}
	typeConverter, err := managedfields.NewTypeConverter(models.Components.Schemas, external.Spec.PreserveUnknownFields)
	if err != nil {
		return result{}, err
	}
	if _, err := typeConverter.ObjectToTyped(resource); err != nil {
		return result{Verdict: "Rejected", Reason: fmt.Sprintf("Kubernetes SSA typed patch validation: %v", err)}, nil
	}

	// Track fields before pruning so strict decoding cannot silently discard a rejection.
	unknown := pruning.PruneWithOptions(resource.Object, structural, true, structuralschema.UnknownFieldPathOptions{TrackUnknownFieldPaths: true})
	defaulting.PruneNonNullableNullsWithoutDefaults(resource.Object, structural)
	metaErr, metaUnknown := objectmeta.CoerceWithOptions(nil, resource.Object, structural, true, objectmeta.CoerceOptions{ReturnUnknownFieldPaths: true})
	unknown = append(unknown, metaUnknown...)
	var errs field.ErrorList
	if metaErr != nil {
		errs = append(errs, metaErr)
	}
	for _, path := range unknown {
		errs = append(errs, &field.Error{Type: field.ErrorTypeInvalid, Field: path, Detail: "unknown field under strict decoding"})
	}
	if len(errs) != 0 {
		return withErrors("Rejected", "strict resource decoding", errs), nil
	}
	defaulting.Default(resource.Object, structural)

	// The create strategy owns ObjectMeta, scale, embedded resources, list uniqueness and CEL.
	strategy := customresource.NewStrategy(nil, crd.Spec.Scope == apiextensions.NamespaceScoped, gvk, schemaValidator, nil, structural, subresources.Status, subresources.Scale)
	strategy.PrepareForCreate(ctx, resource)
	if errs := strategy.Validate(ctx, resource); len(errs) != 0 {
		return withErrors("Rejected", "Kubernetes 1.29 create validation", errs), nil
	}
	effective, err := json.Marshal(resource.Object)
	if err != nil {
		return result{}, err
	}
	return result{Verdict: "Accepted", EffectiveJSON: string(effective), EffectiveSHA256: fmt.Sprintf("%x", sha256.Sum256(effective))}, nil
}

// Helm also installs CRDs through its default SSA Create path.
// Keep one request boundary for declarations and instances.
func applyObject(raw []byte) (*unstructured.Unstructured, error) {
	var resource unstructured.Unstructured
	if err := readJSON(raw, &resource); err != nil {
		return nil, err
	}
	// SSA type-checks the supplied patch before the create strategy resets status.
	if _, supplied := resource.Object["status"]; supplied {
		return nil, fmt.Errorf("supplied status requires unmodelled server-side apply validation")
	}
	// The apply handler and storage have metadata checks outside the native create strategy.
	metadata, _, err := unstructured.NestedMap(resource.Object, "metadata")
	if err != nil {
		return nil, err
	}
	var unsupported []string
	for key := range metadata {
		switch key {
		case "name", "namespace", "labels", "annotations":
		default:
			unsupported = append(unsupported, key)
		}
	}
	sort.Strings(unsupported)
	if len(unsupported) != 0 || resource.GetName() == "" {
		return nil, fmt.Errorf("unmodelled apply/create metadata: fields=%v; a fixed name is required", unsupported)
	}
	if _, _, err := unstructured.NestedString(resource.Object, "metadata", "namespace"); err != nil {
		return nil, err
	}
	return &resource, nil
}

func validatedDeclaration(raw []byte) (*apiextensions.CustomResourceDefinition, error) {
	if _, err := applyObject(raw); err != nil {
		return nil, fmt.Errorf("CRD install: %w", err)
	}
	var declaration apiextensionsv1.CustomResourceDefinition
	if err := readJSON(raw, &declaration); err != nil {
		return nil, err
	}
	if declaration.APIVersion != "apiextensions.k8s.io/v1" || declaration.Kind != "CustomResourceDefinition" {
		return nil, fmt.Errorf("a v1 CustomResourceDefinition declaration is required")
	}
	apiextensionsv1.SetDefaults_CustomResourceDefinition(&declaration)
	var crd apiextensions.CustomResourceDefinition
	if err := apiextensionsv1.Convert_v1_CustomResourceDefinition_To_apiextensions_CustomResourceDefinition(&declaration, &crd, nil); err != nil {
		return nil, err
	}
	if errs := crdvalidation.ValidateCustomResourceDefinition(context.Background(), &crd); len(errs) != 0 {
		sort.Slice(errs, func(i, j int) bool { return errs[i].Error() < errs[j].Error() })
		return nil, fmt.Errorf("invalid CRD: %v", errs.ToAggregate())
	}
	if crd.Spec.Conversion != nil && crd.Spec.Conversion.Strategy != apiextensions.NoneConverter {
		return nil, fmt.Errorf("conversion webhook cannot be evaluated offline")
	}
	return &crd, nil
}

func withErrors(verdict, reason string, errs field.ErrorList) result {
	out := result{Verdict: verdict, Reason: reason}
	for _, err := range errs {
		if err.Type == field.ErrorTypeInternal {
			out.Verdict = "Uncertain"
			out.Reason = "native validation could not execute"
		}
		out.Errors = append(out.Errors, fieldError{Path: err.Field, Type: string(err.Type), Detail: err.Detail})
	}
	sort.Slice(out.Errors, func(i, j int) bool {
		left, right := out.Errors[i], out.Errors[j]
		if left.Path != right.Path {
			return left.Path < right.Path
		}
		if left.Type != right.Type {
			return left.Type < right.Type
		}
		return left.Detail < right.Detail
	})
	return out
}

func readJSON(input []byte, target interface{}) error {
	decoder := json.NewDecoder(bytes.NewReader(input))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return fmt.Errorf("invalid input: %w", err)
	}
	if err := decoder.Decode(new(interface{})); err != io.EOF {
		return fmt.Errorf("expected exactly one JSON document")
	}
	return nil
}
