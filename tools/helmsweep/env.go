package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"slices"

	"helm.sh/helm/v4/pkg/cli"
)

// cleanEnvMarker carries the clean HOME from the re-executing parent to the child.
const cleanEnvMarker = "HELMSWEEP_CLEAN_HOME"

// cleanHome is the empty HOME this process was re-executed with under
// --helm-env-clear, or "" when it was not.
var cleanHome string

// cleanEnvCommands take --helm-env-clear.
var cleanEnvCommands = []string{"lint", "template", "sweep"}

// reexecClean runs a command given --helm-env-clear again, as a child with
// the same arguments under exactly HOME=<fresh empty directory>,
// PATH=/usr/bin:/bin and the caller's temporary-directory variables
// (keptTempVars), and returns its exit code. Clearing the environment
// inside the running process would be too late: client-go reads HOME while
// its packages initialize, before main. done is false in that child, and
// when no clean environment was asked for.
func reexecClean(args []string, stderr io.Writer) (code int, done bool) {
	if home, ok := os.LookupEnv(cleanEnvMarker); ok {
		cleanHome = home
		if err := os.Unsetenv(cleanEnvMarker); err != nil {
			cleanHome = ""
		}
		return 0, false
	}
	if len(args) == 0 || !slices.Contains(cleanEnvCommands, args[0]) ||
		(!slices.Contains(args[1:], "--helm-env-clear") && !slices.Contains(args[1:], "-helm-env-clear")) {
		return 0, false
	}
	exe, err := os.Executable()
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep:", err)
		return exitHarness, true
	}
	home, err := os.MkdirTemp("", "helmsweep-home-")
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep:", err)
		return exitHarness, true
	}
	defer os.RemoveAll(home)
	child := exec.Command(exe, args...)
	child.Env = append(cleanEnv(home), cleanEnvMarker+"="+home)
	child.Stdin, child.Stdout, child.Stderr = os.Stdin, os.Stdout, os.Stderr
	err = child.Run()
	var exit *exec.ExitError
	switch {
	case err == nil:
		return 0, true
	case errors.As(err, &exit) && exit.ExitCode() >= 0:
		return exit.ExitCode(), true
	default:
		fmt.Fprintln(stderr, "helmsweep: clean child:", err)
		return exitHarness, true
	}
}

// keptTempVars survive the clean re-exec, so a clean child's temporary files
// stay where the caller put its own: Go reads TMPDIR on Unix, TMP and TEMP on
// Windows, and GOTMPDIR for the go command.
var keptTempVars = []string{"GOTMPDIR", "TEMP", "TMP", "TMPDIR"}

// cleanEnv is the sorted clean environment: HOME, PATH and those of
// keptTempVars that this process has set.
func cleanEnv(home string) []string {
	env := []string{"HOME=" + home, "PATH=/usr/bin:/bin"}
	for _, name := range keptTempVars {
		if value, ok := os.LookupEnv(name); ok {
			env = append(env, name+"="+value)
		}
	}
	slices.Sort(env)
	return env
}

// requireCleanEnv refuses unless this process is the re-executed child and
// its environment is still exactly the clean one.
func requireCleanEnv() error {
	env := os.Environ()
	slices.Sort(env)
	if cleanHome == "" || !slices.Equal(env, cleanEnv(cleanHome)) {
		return errors.New("--helm-env-clear: the process does not run under the clean environment")
	}
	return nil
}

// unsupportedConfiguration refuses Helm configuration whose CLI code path
// helmsweep does not reproduce, instead of silently differing from the CLI:
// debug output, a storage driver, registry client initialization, the
// repository expiry check and CLI plugin loading.
func unsupportedConfiguration(settings *cli.EnvSettings) error {
	switch {
	case settings.Debug:
		return errors.New("HELM_DEBUG is not reproduced")
	case os.Getenv("HELM_DRIVER") != "":
		return errors.New("HELM_DRIVER is not reproduced")
	}
	for _, path := range []string{settings.RegistryConfig, settings.RepositoryConfig} {
		if _, err := os.Stat(path); err == nil {
			return fmt.Errorf("%s exists: registry and repository initialization is not reproduced", path)
		}
	}
	if entries, err := os.ReadDir(settings.PluginsDirectory); err == nil && len(entries) > 0 {
		return fmt.Errorf("%s holds plugins: CLI plugin loading is not reproduced", settings.PluginsDirectory)
	}
	return nil
}
