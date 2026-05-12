#!/usr/bin/env bash
# scripts/install.sh
#
# Canonical "rebuild + reinstall" workflow. Use this any time you've
# changed source code and want to see the result running in your menu bar.
#
# Equivalent to:
#   ./scripts/build-app-bundle.sh
#   ./scripts/dev-install-app.sh
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/build-app-bundle.sh
./scripts/dev-install-app.sh
