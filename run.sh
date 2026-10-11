#!/usr/bin/env bash
set -euo pipefail
prototype_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$prototype_dir"
exec cargo run --release --locked -- "$@"
