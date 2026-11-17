package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	yamlv2 "go.yaml.in/yaml/v2"
	"sigs.k8s.io/yaml"
)

// Unmarshal can stop while composing aliases before a raw event walk reaches a later error.
// Resolution starts only after composition; reader validation can fail before either stage.
func TestYAMLDiagnosticsFollowPinnedUnmarshalFailureOrder(t *testing.T) {
	cases := []struct {
		name string
		data []byte
	}{
		{"unknown alias before later syntax", []byte("a: *x\nb: [}\n")},
		{"bad scalar tag before later syntax", []byte("a: !!int bad\nb: [}\n")},
		{"invalid byte after first document", append([]byte("a: b\n...\n"), 0xff)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var value any
			actualError := yamlv2.Unmarshal(tc.data, &value)
			expected := yamlv2.ProbeUnmarshalError(tc.data)
			if actualError == nil || expected == nil {
				t.Fatalf("fixture did not exercise an Unmarshal failure: public=%v observed=%v", actualError, expected)
			}
			if actualError.Error() != expected.Message {
				t.Fatalf("observer changed public failure: public=%v observed=%v", actualError, expected.Message)
			}
			result := yamlRead(tc.data, true)
			for target, decoded := range map[string]*yamlResult{"v2": &result.V2, "sigs": &result.Sigs, "map": result.Map, "array": result.Array, "head": result.Head} {
				if decoded == nil || decoded.Error == nil {
					t.Fatalf("%s accepted input rejected by pinned Unmarshal", target)
				}
				if decoded.Error.Kind != expected.Kind || decoded.Error.Line != expected.Line || !strings.HasSuffix(decoded.Error.Message, actualError.Error()) {
					t.Errorf("%s diagnostic=%+v; pinned source diagnostic=%+v", target, decoded.Error, expected)
				}
			}
		})
	}
}

// Tree batches preserve reader and decode outcomes while omitting only retained events.
func TestYAMLTreeBatchPreservesReaderOutcomes(t *testing.T) {
	inputs := [][]byte{
		nil,
		[]byte("a: true\nb: [1, 2]\n...\n[unfinished"),
		[]byte("a: *x\nb: [}\n"),
		[]byte("literal: |\n  one\n  two\n"),
		{0xff},
	}
	data, err := json.Marshal(inputs)
	if err != nil {
		t.Fatal(err)
	}
	response, err := yamlRespond(yamlRequest{Op: "tree_batch", Data: data})
	if err != nil {
		t.Fatal(err)
	}
	actual, ok := response.([]yamlReadResult)
	if !ok || len(actual) != len(inputs) {
		t.Fatalf("tree batch has unexpected response: %T %+v", response, response)
	}
	for index, input := range inputs {
		expected := yamlRead(input, false)
		expected.Probe.Events = nil
		if !reflect.DeepEqual(actual[index], expected) {
			t.Errorf("input %q: tree batch=%+v, full reader without events=%+v", input, actual[index], expected)
		}
	}
}

// Binary string keys can remain distinct in Go while their JSON spellings coincide.
func TestYAMLCanonicalBinaryKeyTiesStayDeterministic(t *testing.T) {
	data := []byte("{!!binary /w==: a, !!binary /g==: b}\n")
	var generic any
	if err := yamlv2.Unmarshal(data, &generic); err != nil {
		t.Fatal(err)
	}
	expected, err := json.Marshal(yamlCanonical(generic))
	if err != nil {
		t.Fatal(err)
	}
	for range 256 {
		actual, err := json.Marshal(yamlCanonical(generic))
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(actual, expected) {
			t.Fatalf("canonical map order changed: have=%s want=%s", actual, expected)
		}
	}

	// A valid replacement rune stays distinct from a malformed byte with the same JSON text.
	var hybrid any
	if err := yamlv2.Unmarshal([]byte("{!!binary /w==: b, \"�\": a}\n"), &hybrid); err != nil {
		t.Fatal(err)
	}
	for _, source := range []any{generic, hybrid} {
		assertCanonicalStringKeysPreserveBytes(t, source)
	}

	// encoding/json sorts the original string bytes before repairing invalid UTF-8.
	rawKeys := map[string]any{string([]byte{0xfe}): "b", string([]byte{0xff}): "a"}
	expectedJSON, err := json.Marshal(rawKeys)
	if err != nil {
		t.Fatal(err)
	}
	actualJSON, err := yaml.YAMLToJSON(data)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(actualJSON, expectedJSON) {
		t.Fatalf("raw key ordering changed: have=%s want=%s", actualJSON, expectedJSON)
	}
	var decoded any
	if err := json.Unmarshal(expectedJSON, &decoded); err != nil {
		t.Fatal(err)
	}
	actual := yamlRead(data, false)
	if actual.Sigs.Error != nil || !reflect.DeepEqual(actual.Sigs.Value, decoded) {
		t.Fatalf("generic decode=%+v, want encoding/json decode=%+v", actual.Sigs, decoded)
	}
}

func assertCanonicalStringKeysPreserveBytes(t *testing.T, source any) {
	t.Helper()
	raw, ok := source.(map[any]any)
	if !ok {
		t.Fatalf("fixture is not a generic Go mapping: %T", source)
	}
	canonical, ok := yamlCanonical(source).(map[string]any)
	if !ok {
		t.Fatalf("canonical mapping is malformed: %T", yamlCanonical(source))
	}
	entries, ok := canonical["entries"].([][]any)
	if !ok || len(entries) != len(raw) {
		t.Fatalf("canonical entries lose Go keys: %+v", canonical)
	}
	expected, err := json.Marshal(canonical)
	if err != nil {
		t.Fatal(err)
	}
	for range 256 {
		actual, err := json.Marshal(yamlCanonical(source))
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(actual, expected) {
			t.Fatalf("canonical entry order changed: have=%s want=%s", actual, expected)
		}
	}
	for _, entry := range entries {
		key, ok := entry[0].(map[string]any)
		if !ok {
			t.Fatalf("canonical key is malformed: %+v", entry)
		}
		original, ok := key["value"].(string)
		if !ok {
			t.Fatalf("canonical string has no spelling: %+v", key)
		}
		if encoded, exists := key["bytes"]; exists {
			text, ok := encoded.(string)
			if !ok {
				t.Fatalf("canonical byte metadata is malformed: %+v", key)
			}
			decoded, err := base64.StdEncoding.DecodeString(text)
			if err != nil {
				t.Fatal(err)
			}
			original = string(decoded)
		}
		value, exists := raw[original]
		if !exists || !reflect.DeepEqual(entry[1], yamlCanonical(value)) {
			t.Fatalf("canonical key does not preserve original Go entry: key=%+v entry=%+v", key, entry)
		}
	}
}
