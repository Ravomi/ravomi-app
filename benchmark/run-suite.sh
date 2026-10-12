#!/usr/bin/env bash
set -euo pipefail
prototype_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$prototype_dir/target/release/project-graph-bevy-canvas"
fixture="$prototype_dir/fixtures/tutorial-shortcut-keys.mio"
result_dir="${1:-$prototype_dir/results/suite}"
backend="${2:-wayland}"
case "$backend" in
  wayland) backend_env=(-u DISPLAY WGPU_BACKEND=vulkan) ;;
  x11) backend_env=(-u WAYLAND_DISPLAY -u WAYLAND_SOCKET WGPU_BACKEND=vulkan WINIT_X11_SCALE_FACTOR=1) ;;
  *) printf 'Backend must be wayland or x11\n' >&2; exit 2 ;;
esac
mkdir -p "$result_dir"
run_case() {
  local name="$1"
  shift
  for repeat in 1 2 3; do
    printf 'Running %s %s/3\n' "$name" "$repeat"
    env "${backend_env[@]}" "$binary" "$fixture" \
      --benchmark --output "$result_dir/$name-$repeat.json" "$@" \
      > "$result_dir/$name-$repeat.log" 2>&1
  done
}
run_case tutorial
run_case tutorial-no-ui --no-ui
run_case tutorial-edit --edit-node 0
run_case tutorial-physics --physics
run_case nodes-1000 --synthetic 1000
run_case nodes-5000 --synthetic 5000
run_case long-images-1000 --synthetic 1000 --long-text --images
run_case images-5000 --synthetic 5000 --images
run_case dense-1000 --synthetic 1000 --dense --physics
run_case physics-5000 --synthetic 5000 --physics
