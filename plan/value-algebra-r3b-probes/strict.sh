#!/usr/bin/env bash
# Helm's CLI does not expose Engine.Strict as a template-render option.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
R3B_GO_BINARY="${R3B_GO:-/Users/roman/.local/share/mise/installs/go/1.26.5/bin/go}"
exec python3 - "$ROOT" "$R3B_GO_BINARY" <<'PY'
import hashlib
import os
import pathlib
import shlex
import subprocess
import sys
import tempfile

root, go = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
assert subprocess.check_output([str(go), "version"], text=True).startswith("go version go1.26.5 ")
modcache = pathlib.Path(subprocess.check_output([str(go), "env", "GOMODCACHE"], text=True).strip())
module = modcache / "helm.sh/helm/v4@v4.2.3"
(root / "target/probes").mkdir(parents=True, exist_ok=True)
scratch = pathlib.Path(tempfile.mkdtemp(prefix="r3b-strict-", dir=root / "target/probes"))
mod = (module / "go.mod").read_text().replace("module helm.sh/helm/v4", "module r3b-strict-probe", 1)
(scratch / "go.mod").write_text(mod + "\nrequire helm.sh/helm/v4 v4.2.3\n")
(scratch / "go.sum").write_bytes((module / "go.sum").read_bytes())
source = root / "plan/value-algebra-r3b-probes/strict.go"
(scratch / "main.go").write_bytes(source.read_bytes())
command = [str(go), "run", "-mod=mod", "."]
print("command:", "env -u GOROOT GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off " + shlex.join(command))
print("cwd: $SCRATCH (unique directory under target/probes)")
print("source_sha256:", hashlib.sha256(source.read_bytes()).hexdigest())
print("upstream_go_mod_sha256:", hashlib.sha256((module / "go.mod").read_bytes()).hexdigest(), flush=True)
env = dict(os.environ, GOTOOLCHAIN="local", GOPROXY="off", GOSUMDB="off")
env.pop("GOROOT", None)
run = subprocess.run(command, cwd=scratch, env=env, check=False)
print("command exit:", run.returncode)
raise SystemExit(run.returncode)
PY
