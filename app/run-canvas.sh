#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir/app"
npm run build:canvas-editor
cd "$project_dir"
cargo build --release --locked --features react-editor --bin project-graph-bevy-canvas
if [[ "$(uname -s)" == Linux ]]; then
  if [[ -z "${DISPLAY:-}" ]]; then
    echo '同窗口 React 编辑目前需要 X11/XWayland 的 DISPLAY。' >&2
    exit 1
  fi
  exec env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET GDK_BACKEND=x11 \
    target/release/project-graph-bevy-canvas "$@" --react-editor
fi
exec target/release/project-graph-bevy-canvas "$@" --react-editor
