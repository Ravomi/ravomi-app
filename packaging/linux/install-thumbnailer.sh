#!/usr/bin/env bash
set -euo pipefail

thumbnail_source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
thumbnail_destdir="${1:-}"
if [[ $# -gt 1 || ( -n "$thumbnail_destdir" && "$thumbnail_destdir" != /* ) ]]; then
  echo "Usage: $0 [absolute-package-staging-directory]" >&2
  exit 2
fi

install -Dm755 "$thumbnail_source_dir/project-graph-thumbnailer.py" "$thumbnail_destdir/usr/libexec/project-graph-bevy-thumbnailer"
install -Dm644 "$thumbnail_source_dir/project-graph.thumbnailer" "$thumbnail_destdir/usr/share/thumbnailers/project-graph-bevy.thumbnailer"
install -Dm644 "$thumbnail_source_dir/project-graph.xml" "$thumbnail_destdir/usr/share/mime/packages/project-graph-bevy.xml"

if [[ -z "$thumbnail_destdir" ]]; then
  update-mime-database /usr/share/mime
fi
