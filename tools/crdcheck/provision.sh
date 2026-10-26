#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# Maintainers opt in; ordinary gates replay recorded results without Go.
export GOTOOLCHAIN=local GOWORK=off GOFLAGS=-mod=readonly
unset GONOSUMDB
export GOCACHE="$PWD/target/crdcheck-go-cache" GOTMPDIR="$PWD/target/tmp" GOMAXPROCS=3
mkdir -p "$GOCACHE" "$GOTMPDIR"
if [[ "$(go version)" != "go version go1.26.5 "* ]]; then
  echo 'crdcheck requires Go 1.26.5 (mise exec go@1.26.5 -- tools/crdcheck/provision.sh)' >&2
  exit 1
fi
go -C tools/crdcheck mod download
go -C tools/crdcheck mod verify
GOPROXY=off GOSUMDB=off go -C tools/crdcheck build -trimpath -buildvcs=false -o "$PWD/target/crdcheck" .
