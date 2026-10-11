#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${1:-$project_dir/target/release/project-graph-bevy-canvas}"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_dir="$HOME/.local/bin"

test -x "$binary"
command -v desktop-file-install >/dev/null
command -v desktop-file-validate >/dev/null
mkdir -p "$bin_dir" "$data_dir/applications"
install -m 755 "$binary" "$bin_dir/graplu.new"
mv -f "$bin_dir/graplu.new" "$bin_dir/graplu"
for size in 32 128 256 512; do
    icon_dir="$data_dir/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 644 "$project_dir/assets/branding/app-icons/${size}x${size}.png" "$icon_dir/ravomi.png"
done

# Desktop Exec quoting has its own escaping rules, independent of shell quoting.
exec_path="$bin_dir/graplu"
exec_path="${exec_path//\\/\\\\\\\\}"
exec_path="${exec_path//\"/\\\"}"
exec_path="${exec_path//\$/\\\$}"
exec_path="${exec_path//\`/\\\`}"
exec_path="${exec_path//%/%%}"
desktop-file-install --dir="$data_dir/applications" \
    --set-key=Exec --set-value="\"$exec_path\" %f" \
    "$project_dir/packaging/linux/io.github.graphif.Graplu.desktop"
desktop-file-validate "$data_dir/applications/io.github.graphif.Graplu.desktop"
update-desktop-database "$data_dir/applications"
if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -f -t "$data_dir/icons/hicolor"
fi
printf 'Installed Ravomi: %s\n' "$bin_dir/graplu"
