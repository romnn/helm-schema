#!/bin/sh
# Stages a build-local copy of this module with Helm v4.2.3 patched by helm-memo.patch (PATCH.md):
# downloads the tagged module (Go verifies it against go.sum), copies it writable to
# $1/third_party/helm-v4.2.3, refuses unless its content digest is the pinned one, applies the
# patch strictly (git apply --check, then git apply: no fuzz, no rejects), and writes a go.mod
# that replaces helm.sh/helm/v4 with the copy. The source tree is never written.
set -eu
out=${1:?build directory}
# sha256 of `shasum -a 256` over every file of helm.sh/helm/v4@v4.2.3, paths in C sort order.
pinned=ad77014f810d103768b7dc7aa3999b076249a315c4b5e436bc1531439867305f
src=$(cd "$(dirname "$0")" && pwd)
cd "$src"
GOFLAGS=-mod=mod go mod download helm.sh/helm/v4@v4.2.3
module=$(GOFLAGS=-mod=mod go list -m -f '{{.Dir}}' helm.sh/helm/v4)
rm -rf "$out"
mkdir -p "$out/third_party"
cp ./*.go go.mod go.sum helm-memo.patch "$out/"
cp -R "$module" "$out/third_party/helm-v4.2.3"
chmod -R u+w "$out/third_party/helm-v4.2.3"
cd "$out/third_party/helm-v4.2.3"
digest=$(find . -type f | LC_ALL=C sort | tr '\n' '\0' | xargs -0 shasum -a 256 | shasum -a 256 | cut -d' ' -f1)
if [ "$digest" != "$pinned" ]; then
  echo "helm v4.2.3 module copy has digest $digest, not the pinned $pinned" >&2
  exit 1
fi
# The ceiling keeps git from treating an enclosing repository as the patch root.
GIT_CEILING_DIRECTORIES="$out/third_party" git apply --check "$out/helm-memo.patch"
GIT_CEILING_DIRECTORIES="$out/third_party" git apply "$out/helm-memo.patch"
printf '\nreplace helm.sh/helm/v4 => ./third_party/helm-v4.2.3\n' >> "$out/go.mod"
