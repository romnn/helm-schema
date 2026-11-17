package main

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"helm.sh/helm/v4/pkg/action"
	"helm.sh/helm/v4/pkg/chart"
	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/chart/loader"
	"helm.sh/helm/v4/pkg/kube"
	releasev1 "helm.sh/helm/v4/pkg/release/v1"
)

type renderResult struct {
	Stdout string  `json:"stdout"`
	Error  *string `json:"error"`
}

// renderDefaults uses the pinned client-only Helm action, without CLI settings or a cluster.
func renderDefaults(path, kubeVersion string) renderResult {
	failure := func(err error) renderResult { text := err.Error(); return renderResult{Error: &text} }
	if !filepath.IsAbs(path) {
		return failure(fmt.Errorf("chart must be an absolute local path"))
	}
	loaded, err := loader.Load(path)
	if err != nil {
		return failure(err)
	}
	accessor, err := chart.NewAccessor(loaded)
	if err != nil {
		return failure(err)
	}
	if kind := accessor.MetadataAsMap()["Type"]; kind != "" && kind != "application" {
		return failure(fmt.Errorf("%s charts are not installable", kind))
	}
	if dependencies := accessor.MetaDependencies(); len(dependencies) > 0 {
		if err := action.CheckDependencies(loaded, dependencies); err != nil {
			return failure(err)
		}
	}
	if kubeVersion == "" {
		kubeVersion = "1.29.0"
	}
	parsed, err := common.ParseKubeVersion(kubeVersion)
	if err != nil {
		return failure(err)
	}
	client := action.NewInstall(action.NewConfiguration())
	client.Timeout = 300 * time.Second
	client.WaitStrategy = kube.HookOnlyStrategy
	client.DryRunStrategy = action.DryRunClient
	client.SkipSchemaValidation = true
	client.Namespace = "default"
	client.ReleaseName = "t"
	client.Replace = true
	client.KubeVersion = parsed
	client.APIVersions = common.VersionSet([]string{})
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	result, err := client.RunWithContext(ctx, loaded, map[string]any{})
	if err != nil {
		return failure(err)
	}
	var release *releasev1.Release
	switch value := result.(type) {
	case releasev1.Release:
		release = &value
	case *releasev1.Release:
		release = value
	default:
		return failure(fmt.Errorf("unsupported release type: %T", result))
	}
	var output strings.Builder
	fmt.Fprintln(&output, strings.TrimSpace(release.Manifest))
	for _, hook := range release.Hooks {
		fmt.Fprintf(&output, "---\n# Source: %s\n%s\n", hook.Path, hook.Manifest)
	}
	return renderResult{Stdout: output.String()}
}
