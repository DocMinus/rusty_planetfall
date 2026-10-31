#!/bin/sh
# Download the Czech Z-machine conformance suite into testdata/ (git-ignored)
# and compile its version 3 build. Needs curl, unzip and Inform 6
# (macOS: brew install inform6).
set -eu
cd "$(dirname "$0")/.."
mkdir -p testdata/dl
cd testdata/dl
curl -sfL -o czech.zip https://www.ifarchive.org/if-archive/infocom/interpreters/tools/czech_0_8.zip
unzip -o -q czech.zip -d czech
cd czech
inform -v3 czech.inf czech.z3
echo "Built testdata/dl/czech/czech.z3"
