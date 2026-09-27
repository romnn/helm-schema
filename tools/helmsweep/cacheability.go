package main

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"text/template/parse"
	"unicode/utf8"
)

// nondeterministicFunctions are the template functions whose result can
// differ between identical invocations: the list of
// crates/helm-schema/tests/common/helm_cache_policy.rs.
var nondeterministicFunctions = []string{
	// Clocks.
	"ago", "date", "dateInZone", "dateModify", "date_in_zone", "date_modify", "htmlDate", "htmlDateInZone",
	"mustDateModify", "must_date_modify", "now", "unixEpoch",
	// Randomness and salted or keyed generation.
	"bcrypt", "derivePassword", "encryptAES", "genCA", "genCAWithKey", "genPrivateKey", "genSelfSignedCert",
	"genSelfSignedCertWithKey", "genSignedCert", "genSignedCertWithKey", "htpasswd", "randAlpha", "randAlphaNum",
	"randAscii", "randBytes", "randInt", "randNumeric", "shuffle", "uuidv4",
	// Unordered map iteration.
	"keys", "values",
	// Text executed at render time.
	"tpl",
	// Answers from outside the chart.
	"getHostByName",
}

// chartCacheability reports whether identical lint or template invocations
// of the chart copy at dir must reach identical verdicts, else why not.
// Every file Helm executes — under a templates directory of the chart or of
// a dependency, unpacked or packaged at any depth — is parsed with
// text/template/parse, the parser Helm's engine uses. A call to one of
// nondeterministicFunctions, or a template that does not parse, makes the
// chart uncacheable. `lookup` does not: lint and client-only template
// answer it empty.
func chartCacheability(dir string) (bool, string, error) {
	var reasons []string
	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || !d.Type().IsRegular() {
			return err
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return scanChartFile(filepath.ToSlash(rel), data, &reasons)
	})
	if err != nil {
		return false, "", err
	}
	if len(reasons) == 0 {
		return true, "", nil
	}
	slices.Sort(reasons)
	return false, reasons[0], nil
}

func scanChartFile(name string, data []byte, reasons *[]string) error {
	if strings.HasSuffix(name, ".tgz") || strings.HasSuffix(name, ".tar.gz") {
		unzipped, err := gzip.NewReader(bytes.NewReader(data))
		if err != nil {
			return fmt.Errorf("%s: %w", name, err)
		}
		archive := tar.NewReader(unzipped)
		for {
			header, err := archive.Next()
			if err == io.EOF {
				return nil
			}
			if err != nil {
				return fmt.Errorf("%s: %w", name, err)
			}
			if header.Typeflag != tar.TypeReg {
				continue
			}
			member, err := io.ReadAll(archive)
			if err != nil {
				return fmt.Errorf("%s: %w", name, err)
			}
			if err := scanChartFile(name+":"+header.Name, member, reasons); err != nil {
				return err
			}
		}
	}
	isTemplate := slices.Contains(strings.FieldsFunc(name, func(r rune) bool { return r == '/' || r == ':' }), "templates")
	if isTemplate {
		scanTemplate(name, data, reasons)
	}
	return nil
}

func scanTemplate(name string, data []byte, reasons *[]string) {
	if !utf8.Valid(data) {
		*reasons = append(*reasons, name+": template is not UTF-8")
		return
	}
	tree := parse.New(name)
	tree.Mode = parse.SkipFuncCheck | parse.ParseComments
	trees := map[string]*parse.Tree{}
	if _, err := tree.Parse(string(data), "", "", trees); err != nil {
		*reasons = append(*reasons, fmt.Sprintf("%s: template does not parse: %v", name, err))
		return
	}
	trees[""] = tree
	for _, t := range trees {
		walkTemplate(t.Root, func(function string) {
			if slices.Contains(nondeterministicFunctions, function) {
				*reasons = append(*reasons, name+": calls "+function)
			}
		})
	}
}

// walkTemplate calls visit with every function identifier under node.
func walkTemplate(node parse.Node, visit func(string)) {
	switch n := node.(type) {
	case *parse.ListNode:
		if n != nil {
			for _, child := range n.Nodes {
				walkTemplate(child, visit)
			}
		}
	case *parse.ActionNode:
		walkTemplate(n.Pipe, visit)
	case *parse.PipeNode:
		if n != nil {
			for _, command := range n.Cmds {
				walkTemplate(command, visit)
			}
		}
	case *parse.CommandNode:
		for _, arg := range n.Args {
			walkTemplate(arg, visit)
		}
	case *parse.ChainNode:
		walkTemplate(n.Node, visit)
	case *parse.IdentifierNode:
		visit(n.Ident)
	case *parse.IfNode:
		walkBranch(&n.BranchNode, visit)
	case *parse.RangeNode:
		walkBranch(&n.BranchNode, visit)
	case *parse.WithNode:
		walkBranch(&n.BranchNode, visit)
	case *parse.TemplateNode:
		walkTemplate(n.Pipe, visit)
	}
}

func walkBranch(n *parse.BranchNode, visit func(string)) {
	walkTemplate(n.Pipe, visit)
	walkTemplate(n.List, visit)
	walkTemplate(n.ElseList, visit)
}
