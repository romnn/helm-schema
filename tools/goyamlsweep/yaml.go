package main

import (
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"sort"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/Masterminds/sprig/v3"
	yamlv2 "go.yaml.in/yaml/v2"
	"helm.sh/helm/v4/pkg/release/v1/util"
	"sigs.k8s.io/yaml"
)

var yamlFunctions = sprig.TxtFuncMap()

var (
	yamlIndent  = yamlFunctions["indent"].(func(int, string) string)
	yamlNindent = yamlFunctions["nindent"].(func(int, string) string)
	yamlTrim    = yamlFunctions["trim"].(func(string) string)
	yamlQuote   = yamlFunctions["quote"].(func(...any) string)
)

type yamlRequest struct {
	Op          string          `json:"op"`
	Data        json.RawMessage `json:"data"`
	Text        string          `json:"text"`
	Width       int             `json:"width"`
	From        uint32          `json:"from"`
	To          uint32          `json:"to"`
	Chart       string          `json:"chart"`
	KubeVersion string          `json:"kube_version"`
}

type yamlError struct {
	Kind    string `json:"kind"`
	Line    int    `json:"line"`
	Message string `json:"message"`
}

type yamlResult struct {
	Value any        `json:"value"`
	Error *yamlError `json:"error"`
}

type yamlReadResult struct {
	V2    yamlResult         `json:"v2"`
	Sigs  yamlResult         `json:"sigs"`
	Map   *yamlResult        `json:"map,omitempty"`
	Array *yamlResult        `json:"array,omitempty"`
	Head  *yamlResult        `json:"head,omitempty"`
	Probe yamlv2.ReaderProbe `json:"probe"`
}

type yamlStageResult struct {
	Split   []string `json:"split"`
	Indent  string   `json:"indent"`
	Nindent string   `json:"nindent"`
	Trim    string   `json:"trim"`
	Quote   string   `json:"quote"`
	Strip   string   `json:"strip"`
}

// cmdYAML exposes the pinned readers and stages without interpreting their input.
func cmdYAML(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) != 0 {
		fmt.Fprintln(stderr, "usage: goyamlsweep [yaml] < requests.jsonl")
		return exitRefused
	}
	decoder := json.NewDecoder(stdin)
	encoder := json.NewEncoder(stdout)
	encoder.SetEscapeHTML(false)
	for {
		var request yamlRequest
		if err := decoder.Decode(&request); err != nil {
			if errors.Is(err, io.EOF) {
				return 0
			}
			fmt.Fprintln(stderr, "goyamlsweep:", err)
			return exitHarness
		}
		response, err := yamlRespond(request)
		if err != nil {
			fmt.Fprintln(stderr, "goyamlsweep:", err)
			return exitRefused
		}
		if err := encoder.Encode(response); err != nil {
			return exitHarness
		}
	}
}

func yamlRespond(request yamlRequest) (any, error) {
	switch request.Op {
	case "unicode":
		if request.From > request.To || request.To > 0x110000 || request.To-request.From > 4096 {
			return nil, fmt.Errorf("unicode range must be ordered, at most 4096 code points, and within 0..0x110000")
		}
		result := make([]unicodeResult, 0, request.To-request.From)
		for value := request.From; value < request.To; value++ {
			if value >= 0xd800 && value <= 0xdfff {
				continue
			}
			ch := rune(value) //nolint:gosec // G115: The checked range ends at 0x110000, below MaxInt32.
			result = append(result, unicodeResult{Codepoint: value, Quote: yamlQuote(string(ch)), Space: unicode.IsSpace(ch)})
		}
		return result, nil
	case "render":
		return renderDefaults(request.Chart, request.KubeVersion), nil
	case "read":
		var data []byte
		if err := json.Unmarshal(request.Data, &data); err != nil {
			return nil, err
		}
		return yamlRead(data, true), nil
	case "read_batch", "v2_batch", "tree_batch":
		var inputs [][]byte
		if err := json.Unmarshal(request.Data, &inputs); err != nil {
			return nil, err
		}
		output := make([]yamlReadResult, 0, len(inputs))
		for _, input := range inputs {
			output = append(output, yamlReadMode(input, request.Op == "read_batch", request.Op != "tree_batch"))
		}
		return output, nil
	case "stages":
		if request.Width < 0 {
			return nil, fmt.Errorf("negative indent width: %d", request.Width)
		}
		return yamlStages(request.Text, request.Width), nil
	case "split_batch":
		var inputs []string
		if err := json.Unmarshal(request.Data, &inputs); err != nil {
			return nil, err
		}
		output := make([][]string, 0, len(inputs))
		for _, input := range inputs {
			output = append(output, yamlSplit(input))
		}
		return output, nil
	case "stages_batch":
		if request.Width < 0 {
			return nil, fmt.Errorf("negative indent width: %d", request.Width)
		}
		var inputs []string
		if err := json.Unmarshal(request.Data, &inputs); err != nil {
			return nil, err
		}
		output := make([]yamlStageResult, 0, len(inputs))
		for _, input := range inputs {
			output = append(output, yamlStages(input, request.Width))
		}
		return output, nil
	default:
		return nil, fmt.Errorf("unknown yaml operation %q", request.Op)
	}
}

func yamlRead(data []byte, allTargets bool) yamlReadResult {
	return yamlReadMode(data, allTargets, true)
}

func yamlReadMode(data []byte, allTargets, events bool) yamlReadResult {
	var probe yamlv2.ReaderProbe
	if events {
		probe = yamlv2.ProbeReader(data)
	} else {
		probe = yamlv2.ProbeReaderTree(data)
	}
	var generic any
	err := yamlv2.Unmarshal(data, &generic)
	var source *yamlv2.ProbeError
	if err != nil {
		if probe.Error != nil && probe.Error.Message != "" && strings.HasSuffix(err.Error(), probe.Error.Message) {
			source = &yamlv2.ProbeError{Kind: probe.Error.Kind, Line: probe.Error.Line, Message: err.Error()}
		} else {
			source = yamlv2.ProbeUnmarshalError(data)
		}
	}
	result := yamlReadResult{
		V2:    yamlResult{Value: yamlCanonical(generic), Error: yamlDiagnostic(err, source)},
		Probe: probe,
	}
	var converted any
	err = yaml.Unmarshal(data, &converted)
	result.Sigs = yamlResult{Value: converted, Error: yamlDiagnostic(err, source)}
	if !allTargets {
		return result
	}
	var object map[string]any
	err = yaml.Unmarshal(data, &object)
	result.Map = &yamlResult{Value: object, Error: yamlDiagnostic(err, source)}
	var array []any
	err = yaml.Unmarshal(data, &array)
	result.Array = &yamlResult{Value: array, Error: yamlDiagnostic(err, source)}
	var head util.SimpleHead
	err = yaml.Unmarshal(data, &head)
	result.Head = &yamlResult{Value: head, Error: yamlDiagnostic(err, source)}
	return result
}

func yamlDiagnostic(err error, source *yamlv2.ProbeError) *yamlError {
	if err == nil {
		return nil
	}
	kind := "decode"
	line := 0
	if source != nil && strings.HasSuffix(err.Error(), source.Message) {
		kind = source.Kind
		line = source.Line
	} else if strings.Contains(err.Error(), "error unmarshaling JSON") || strings.Contains(err.Error(), "error converting YAML to JSON") {
		kind = "json"
	}
	return &yamlError{Kind: kind, Line: line, Message: err.Error()}
}

func yamlCanonical(value any) any {
	switch value := value.(type) {
	case nil:
		return map[string]any{"kind": "null"}
	case bool:
		return map[string]any{"kind": "bool", "value": value}
	case int:
		return map[string]any{"kind": "int", "value": strconv.FormatInt(int64(value), 10)}
	case int64:
		return map[string]any{"kind": "int", "value": strconv.FormatInt(value, 10)}
	case uint64:
		return map[string]any{"kind": "uint", "value": strconv.FormatUint(value, 10)}
	case float64:
		return map[string]any{"kind": "float", "value": fmt.Sprintf("%016x", math.Float64bits(value))}
	case string:
		// JSON's repaired spelling cannot preserve Go strings' raw byte identity.
		if !utf8.ValidString(value) {
			return map[string]any{"kind": "string", "value": string([]rune(value)), "bytes": base64.StdEncoding.EncodeToString([]byte(value))}
		}
		return map[string]any{"kind": "string", "value": value}
	case []any:
		items := make([]any, 0, len(value))
		for _, item := range value {
			items = append(items, yamlCanonical(item))
		}
		return map[string]any{"kind": "sequence", "items": items}
	case map[any]any:
		entries := make([][]any, 0, len(value))
		for key, item := range value {
			entries = append(entries, []any{yamlCanonical(key), yamlCanonical(item)})
		}
		// Sort complete entries to preserve deterministic order even when canonical keys tie.
		sort.Slice(entries, func(i, j int) bool {
			left, _ := json.Marshal(entries[i])
			right, _ := json.Marshal(entries[j])
			return string(left) < string(right)
		})
		return map[string]any{"kind": "mapping", "entries": entries}
	default:
		return map[string]any{"kind": fmt.Sprintf("unsupported:%T", value), "value": fmt.Sprint(value)}
	}
}

// yamlStages calls Sprig's actual registered functions and Helm's actual splitter.
func yamlStages(text string, width int) yamlStageResult {
	return yamlStageResult{
		Split: yamlSplit(text), Indent: yamlIndent(width, text), Nindent: yamlNindent(width, text),
		Trim: yamlTrim(text), Quote: yamlQuote(text), Strip: strings.ReplaceAll(text, "<no value>", ""),
	}
}

func yamlSplit(text string) []string {
	manifests := util.SplitManifests(text)
	chunks := make([]string, 0, len(manifests))
	for index := 0; index < len(manifests); index++ {
		chunks = append(chunks, manifests[fmt.Sprintf("manifest-%d", index)])
	}
	return chunks
}

// unicodeResult observes the pinned Go Unicode predicates and quote operation.
type unicodeResult struct {
	Codepoint uint32 `json:"codepoint"`
	Quote     string `json:"quote"`
	Space     bool   `json:"space"`
}
