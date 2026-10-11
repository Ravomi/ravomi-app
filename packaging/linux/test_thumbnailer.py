"""Offline regressions for preview framing and output resolution."""

import hashlib
import importlib.util
import json
import os
import struct
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

import cairo

MODULE_PATH = Path(__file__).with_name("project-graph-thumbnailer.py")
SPEC = importlib.util.spec_from_file_location("thumbnailer", MODULE_PATH)
thumbnailer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(thumbnailer)


class ThumbnailTests(unittest.TestCase):
    def test_sqlite_preview_is_read_only_and_checks_content_and_version(self):
        import sqlite3

        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "project.mio")
            output = Path(directory, "preview.png")
            conn = sqlite3.connect(source)
            conn.executescript(
                (MODULE_PATH.parents[2] / "src/project_sqlite.sql").read_text()
            )
            conn.execute("PRAGMA application_id=1296650035")
            conn.execute("PRAGMA user_version=3")
            manifest = {
                "format": "ravomi-project",
                "schemaVersion": 3,
                "canvasSchemaVersion": 1,
                "requiredFeatures": ["sqlite-graph-v1"],
                "projectId": "90d4ef64-087e-476b-8a2a-dfd03f1b9e59",
            }
            conn.execute(
                "INSERT INTO project_meta VALUES(1,?,0)",
                (json.dumps(manifest).encode(),),
            )
            stage = [
                {
                    "_": "TextNode",
                    "uuid": "note",
                    "text": "SQLite 画布",
                    "collisionBox": {
                        "shapes": [
                            {
                                "_": "Rectangle",
                                "location": {"x": 0, "y": 0},
                                "size": {"x": 160, "y": 80},
                            }
                        ]
                    },
                }
            ]
            for data in [b"[null]", json.dumps(stage[0]).encode()]:
                conn.execute(
                    "INSERT INTO objects VALUES(?,?)",
                    (hashlib.sha256(data).hexdigest(), data),
                )
            conn.execute(
                "INSERT INTO canvas_root VALUES(1,?)",
                (hashlib.sha256(b"[null]").hexdigest(),),
            )
            conn.execute(
                "INSERT INTO canvas_records VALUES('note','/0','TextNode',?)",
                (hashlib.sha256(json.dumps(stage[0]).encode()).hexdigest(),),
            )
            conn.commit()
            before = source.read_bytes()
            thumbnailer.render(source, output, 64)
            self.assertTrue(output.exists())
            self.assertEqual(source.read_bytes(), before)
            conn.execute(
                "UPDATE objects SET data=? WHERE hash=?",
                (b"damaged", hashlib.sha256(b"[null]").hexdigest()),
            )
            conn.commit()
            with self.assertRaises(ValueError):
                thumbnailer.sqlite_preview(source)
            conn.execute("PRAGMA user_version=999")
            with self.assertRaises(ValueError):
                thumbnailer.sqlite_preview(source)
            conn.close()

    def test_mio_preview_reads_native_canvas_and_assets_without_touching_source(self):
        import io

        import msgpack

        stage = [
            {
                "_": "ImageNode",
                "uuid": "image",
                "attachmentId": "picture",
                "collisionBox": {
                    "shapes": [
                        {
                            "_": "Rectangle",
                            "location": {"x": 0, "y": 0},
                            "size": {"x": 100, "y": 100},
                        }
                    ]
                },
            }
        ]
        manifest = {
            "format": "ravomi-project",
            "schemaVersion": 1,
            "projectId": "90d4ef64-087e-476b-8a2a-dfd03f1b9e59",
            "canvasSchemaVersion": 1,
            "requiredFeatures": [],
        }
        surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, 100, 100)
        context = cairo.Context(surface)
        context.set_source_rgb(1, 0, 0)
        context.paint()
        png = io.BytesIO()
        surface.write_to_png(png)
        with tempfile.TemporaryDirectory() as directory:
            source, output = (
                Path(directory, "project.mio"),
                Path(directory, "preview.png"),
            )
            for version in (1, 2):
                with self.subTest(schema_version=version):
                    manifest["schemaVersion"] = version
                    manifest["requiredFeatures"] = (
                        [] if version == 1 else ["portable-snapshot-v1"]
                    )
                    history_entry = (
                        "collaboration/history.json"
                        if version == 1
                        else "collaboration/records/history.msgpack"
                    )
                    with zipfile.ZipFile(source, "w") as archive:
                        archive.writestr("manifest.json", json.dumps(manifest))
                        archive.writestr("canvas/stage.msgpack", msgpack.packb(stage))
                        archive.writestr("assets/picture.png", png.getvalue())
                        # Collaboration validation belongs to the editor. Its
                        # records are never decoded while rendering a preview.
                        archive.writestr(history_entry, b"not read for previews")
                        if version == 2:
                            archive.writestr(
                                "collaboration/snapshot.json", b"not read for previews"
                            )
                            archive.writestr(
                                "objects/sha256/" + "a" * 64, b"not read for previews"
                            )
                    before = source.read_bytes()
                    thumbnailer.render(str(source), str(output), 128)
                    self.assertEqual(source.read_bytes(), before)
                    self.assertGreater(
                        sum(
                            pixel == (255, 0, 0)
                            for pixel in self.pixels(output.read_bytes())
                        ),
                        1000,
                    )

    def test_mio_preview_rejects_unsupported_manifest_without_legacy_fallback(self):
        manifest = {
            "format": "ravomi-project",
            "schemaVersion": 2,
            "canvasSchemaVersion": 1,
            "projectId": "90d4ef64-087e-476b-8a2a-dfd03f1b9e59",
            "requiredFeatures": ["portable-snapshot-v1"],
        }
        invalid = [
            {"schemaVersion": 3},
            {"schemaVersion": 0},
            {"schemaVersion": True},
            {"canvasSchemaVersion": 2},
            {"canvasSchemaVersion": True},
            {"requiredFeatures": ["portable-snapshot-v1", "future-feature"]},
            {"requiredFeatures": []},
            {"requiredFeatures": "portable-snapshot-v1"},
            {"requiredFeatures": None},
            {"schemaVersion": 1, "requiredFeatures": ["future-feature"]},
        ]
        with tempfile.TemporaryDirectory() as directory:
            source, output = (
                Path(directory, "project.mio"),
                Path(directory, "preview.png"),
            )
            for changes in invalid:
                with self.subTest(changes=changes):
                    with zipfile.ZipFile(source, "w") as archive:
                        archive.writestr(
                            "manifest.json", json.dumps({**manifest, **changes})
                        )
                        archive.writestr(
                            "stage.json",
                            json.dumps({"objects": [self.node("old", "stale")]}),
                        )
                    output.write_bytes(b"previous preview")
                    with self.assertRaisesRegex(
                        ValueError, "Unsupported Ravomi project"
                    ):
                        thumbnailer.render(str(source), str(output), 128)
                    self.assertEqual(output.read_bytes(), b"previous preview")

    def native_document(self, directory, graph):
        source = Path(directory, "native.prg")
        blocks = {}
        data = bytearray(b"\0" * 32)
        for name, kind, payload in (
            ("records", "variant", b"not-decoded-by-thumbnailer"),
            ("preview", "json", json.dumps(graph).encode()),
        ):
            blocks[name] = {
                "offset": len(data),
                "length": len(payload),
                "kind": kind,
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
            data.extend(payload)
        manifest = json.dumps(
            {"version": 4, "codec": "godot-variant-4", "blocks": blocks}
        ).encode()
        data[:32] = struct.pack(
            "<8sIIQQ", thumbnailer.NATIVE_MAGIC, 4, len(manifest), len(data), 0
        )
        data.extend(manifest)
        source.write_bytes(data)
        return source, blocks

    def test_native_preview_does_not_decode_records(self):
        graph = {"objects": [self.node("a", "Native preview", x=20, y=30)]}
        with tempfile.TemporaryDirectory() as directory:
            source, _ = self.native_document(directory, graph)
            loaded, assets = thumbnailer.native_preview(str(source))
            self.assertEqual(loaded, graph)
            self.assertEqual(assets, {})
            output = Path(directory, "native.png")
            with patch.object(thumbnailer, "prefers_light_theme", return_value=False):
                thumbnailer.render(str(source), str(output), 256)
            image = cairo.ImageSurface.create_from_png(str(output))
            self.assertEqual((image.get_width(), image.get_height()), (512, 512))

    def test_native_preview_rejects_checksum_damage(self):
        with tempfile.TemporaryDirectory() as directory:
            source, blocks = self.native_document(directory, {"objects": []})
            data = bytearray(source.read_bytes())
            data[blocks["preview"]["offset"]] ^= 1
            source.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "Damaged PRG preview block"):
                thumbnailer.native_preview(str(source))

    def test_native_preview_rejects_truncated_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            source, _ = self.native_document(directory, {"objects": []})
            source.write_bytes(source.read_bytes()[:-1])
            with self.assertRaisesRegex(ValueError, "Invalid PRG directory"):
                thumbnailer.native_preview(str(source))

    def render_graph(self, objects, light=False):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "graph.prg")
            output = Path(directory, "graph.png")
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr("stage.json", json.dumps({"objects": objects}))
            with patch.object(thumbnailer, "prefers_light_theme", return_value=light):
                thumbnailer.render(str(source), str(output), 256)
            return output.read_bytes()

    def pixels(self, png):
        import io
        import sys

        surface = cairo.ImageSurface.create_from_png(io.BytesIO(png))
        data = surface.get_data()
        return [
            tuple(int.from_bytes(data[i : i + 4], sys.byteorder).to_bytes(4, "big")[1:])
            for i in range(0, len(data), 4)
        ]

    def node(self, identifier, text, parent="", fill=(1, 1, 1, 0), x=0, y=0):
        props = {
            "id": "s:" + identifier,
            "text": "s:" + text,
            "font_size": "i:24",
            "fill_color": {"type": "Color", "args": list(fill)},
            "text_color": {"type": "Color", "args": [1, 1, 1, 0]},
        }
        if parent:
            props["container"] = {"$ref": parent}
        return {
            "type": "text_node",
            "properties": props,
            "transform": {"position": [x, y]},
        }

    def test_saved_purple_fill_and_dark_text_match_canvas(self):
        # Same serialization and containment as Desktop/project/未命名.prg.
        png = self.render_graph(
            [
                self.node("parent", "中心主题"),
                self.node(
                    "child", "你好", "parent", (203 / 255, 166 / 255, 247 / 255, 1)
                ),
            ]
        )
        pixels = self.pixels(png)
        self.assertGreater(sum(p == (203, 166, 247) for p in pixels), 10000)
        purple = [
            (i % 512, i // 512) for i, p in enumerate(pixels) if p == (203, 166, 247)
        ]
        left, right = min(x for x, y in purple), max(x for x, y in purple)
        top, bottom = min(y for x, y in purple), max(y for x, y in purple)
        self.assertGreater(
            sum(
                max(pixels[y * 512 + x]) < 65
                for y in range(top + 20, bottom - 20)
                for x in range(left + 20, right - 20)
            ),
            300,
        )

    def test_transparent_node_keeps_canvas_background(self):
        pixels = self.pixels(self.render_graph([self.node("leaf", "")]))
        self.assertEqual(pixels[256 * 512 + 256], (30, 30, 46))

    def test_nested_fills_are_order_independent_and_use_layer_opacity(self):
        blue = (137 / 255, 180 / 255, 250 / 255, 1)
        outer = self.node("outer", "Outer", fill=blue)
        inner = self.node("inner", "Inner", "outer", blue)
        leaf = self.node("leaf", "Leaf", "inner", blue)
        first = self.render_graph([outer, inner, leaf])
        self.assertEqual(first, self.render_graph([leaf, inner, outer]))
        pixels = self.pixels(first)
        self.assertGreater(sum(p == (137, 180, 250) for p in pixels), 1000)
        # Outer: alpha .78**2, blended over #1e1e2e, not an opaque blue.
        expected = tuple(
            round(a * 0.78**2 + b * (1 - 0.78**2))
            for a, b in zip((137, 180, 250), (30, 30, 46))
        )
        self.assertGreater(
            sum(all(abs(a - b) <= 1 for a, b in zip(p, expected)) for p in pixels), 1000
        )

    def test_partial_alpha_blends_over_canvas_in_both_themes(self):
        for light, canvas in [(False, (30, 30, 46)), (True, (239, 241, 245))]:
            with self.subTest(light=light):
                node = self.node("leaf", "", fill=(1, 0, 0, 0.5))
                pixels = self.pixels(self.render_graph([node], light=light))
                expected = tuple(
                    round(a * 0.5 + b * 0.5) for a, b in zip((255, 0, 0), canvas)
                )
                self.assertTrue(
                    all(
                        abs(a - b) <= 1
                        for a, b in zip(pixels[256 * 512 + 256], expected)
                    )
                )

    def test_saved_text_color_uses_neutral_background_contrast(self):
        for light in (False, True):
            for fill in ((0, 0, 0, 0), (1, 1, 1, 1), (0, 0, 0, 1)):
                with self.subTest(light=light, fill=fill):
                    node = self.node("leaf", "你好", fill=fill)
                    node["properties"]["text_color"]["args"] = [1, 0, 0, 0.2]
                    automatic = self.node("leaf", "你好", fill=fill)
                    self.assertEqual(
                        self.render_graph([node], light=light),
                        self.render_graph([automatic], light=light),
                    )

    def test_node_text_does_not_wrap_at_preview_width(self):
        node = self.node("leaf", "Project Graph " * 8)
        node["properties"]["fixed_width"] = "f:40.0"
        layouts = []
        original = thumbnailer.layout_for

        def capture(*args):
            layout = original(*args)
            layouts.append(layout)
            return layout

        with patch.object(thumbnailer, "layout_for", side_effect=capture):
            self.render_graph([node])
        self.assertEqual(layouts[0].get_line_count(), 1)

    def test_preview_includes_saved_edge_caption_and_arrow(self):
        source = self.node("source", "Source")
        target = self.node("target", "Target", x=350)
        edge = {
            "type": "line_edge",
            "properties": {
                "source": {"$ref": "source"},
                "target": {"$ref": "target"},
                "text": "s:你好",
                "show_arrow": True,
                "use_theme_color": True,
            },
        }
        labels = []
        original = thumbnailer.layout_for

        def capture(ctx, text, size):
            labels.append(text)
            return original(ctx, text, size)

        with patch.object(thumbnailer, "layout_for", side_effect=capture):
            arrow = self.render_graph([source, target, edge])
        self.assertIn("你好", labels)
        edge["properties"]["show_arrow"] = False
        self.assertNotEqual(arrow, self.render_graph([source, target, edge]))

    def test_master_paths_restore_nested_group_and_edge(self):
        stage = [
            {
                "_": "TextNode",
                "uuid": "a",
                "text": "你好",
                "collisionBox": {
                    "shapes": [
                        {
                            "_": "Rectangle",
                            "location": {"x": 50, "y": 80},
                            "size": {"x": 120, "y": 60},
                        }
                    ]
                },
            },
            {"_": "Section", "uuid": "b", "text": "分组", "children": [{"$": "/0"}]},
            {
                "_": "LineEdge",
                "uuid": "c",
                "associationList": [{"$": "/1"}, {"$": "/0"}],
            },
        ]
        graph = thumbnailer.legacy_graph(stage)
        self.assertEqual(len(graph["objects"]), 3)
        self.assertEqual(graph["objects"][0]["properties"]["container"], {"$ref": "b"})
        self.assertEqual(graph["objects"][2]["properties"]["target"], {"$ref": "a"})
        with tempfile.TemporaryDirectory() as directory:
            import msgpack

            source, output = Path(directory, "old.prg"), Path(directory, "old.png")
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr("stage.msgpack", msgpack.packb(stage))
            thumbnailer.render(str(source), str(output), 128)
            surface = cairo.ImageSurface.create_from_png(str(output))
            self.assertEqual(surface.get_width(), 256)

    def test_embedded_preview_is_used_when_no_stage_is_available(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory, "old.prg"), Path(directory, "old.png")
            import io

            original = cairo.ImageSurface(cairo.FORMAT_ARGB32, 100, 100)
            ctx = cairo.Context(original)
            ctx.set_source_rgb(1, 0, 0)
            ctx.paint()
            png = io.BytesIO()
            original.write_to_png(png)
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr("thumbnail.png", png.getvalue())
            thumbnailer.render(str(source), str(output), 256)
            self.assertEqual(
                self.pixels(output.read_bytes())[256 * 512 + 256], (255, 0, 0)
            )

    def test_saved_stage_takes_precedence_over_stale_embedded_preview(self):
        import io

        import msgpack

        with tempfile.TemporaryDirectory() as directory:
            source, output = (
                Path(directory, "edited.prg"),
                Path(directory, "edited.png"),
            )
            original = cairo.ImageSurface(cairo.FORMAT_ARGB32, 40, 40)
            ctx = cairo.Context(original)
            ctx.set_source_rgb(1, 0, 0)
            ctx.paint()
            png = io.BytesIO()
            original.write_to_png(png)
            rendered = []
            for text in ["原来的文字", "编辑之后的新内容"]:
                stage = [
                    {
                        "_": "TextNode",
                        "uuid": "node",
                        "text": text,
                        "collisionBox": {
                            "shapes": [
                                {
                                    "_": "Rectangle",
                                    "location": {"x": 0, "y": 0},
                                    "size": {"x": 500, "y": 100},
                                }
                            ]
                        },
                    }
                ]
                with zipfile.ZipFile(source, "w") as archive:
                    archive.writestr("stage.msgpack", msgpack.packb(stage))
                    archive.writestr("thumbnail.png", png.getvalue())
                before = source.read_bytes()
                with patch.object(
                    thumbnailer, "prefers_light_theme", return_value=False
                ):
                    thumbnailer.render(str(source), str(output), 256)
                self.assertEqual(source.read_bytes(), before)
                rendered.append(output.read_bytes())
                self.assertEqual(
                    sum(p == (255, 0, 0) for p in self.pixels(rendered[-1])), 0
                )
            self.assertNotEqual(*rendered)

    def test_source_document_cannot_be_overwritten_via_same_path_or_link(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "source.prg")
            source.write_bytes(b"keep document")
            alias = Path(directory, "alias.png")
            alias.hardlink_to(source)
            for output in [source, alias]:
                with self.assertRaisesRegex(ValueError, "must not replace"):
                    thumbnailer.render(str(source), str(output), 256)
            self.assertEqual(source.read_bytes(), b"keep document")

    def test_cli_preserves_existing_output_on_bad_document(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = (
                Path(directory, "损坏 文件.prg"),
                Path(directory, "out.png"),
            )
            source.write_bytes(b"invalid document")
            output.write_bytes(b"previous output")
            result = subprocess.run(
                ["/usr/bin/python3", str(MODULE_PATH), str(source), str(output), "256"],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("Project Graph thumbnail:", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertEqual(output.read_bytes(), b"previous output")

    def test_staged_install_and_mime_registration_match_desktop_contract(self):
        import configparser

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(
                [
                    "bash",
                    str(MODULE_PATH.with_name("install-thumbnailer.sh")),
                    directory,
                ],
                check=True,
                capture_output=True,
            )
            helper = root / "usr/libexec/project-graph-bevy-thumbnailer"
            self.assertTrue(os.access(helper, os.X_OK))
            self.assertEqual(helper.read_bytes(), MODULE_PATH.read_bytes())
            entry = configparser.ConfigParser(interpolation=None)
            entry.read(root / "usr/share/thumbnailers/project-graph-bevy.thumbnailer")
            section = entry["Thumbnailer Entry"]
            self.assertEqual(
                section["TryExec"], "/usr/libexec/project-graph-bevy-thumbnailer"
            )
            self.assertEqual(section["Exec"], section["TryExec"] + " %i %o %s")
            self.assertEqual(
                set(section["MimeType"].strip(";").split(";")),
                {
                    "application/x-ravomi",
                    "application/vnd.project-graph",
                    "application/x-project-graph",
                },
            )
            mime = root / "usr/share/mime"
            subprocess.run(
                ["update-mime-database", str(mime)], check=True, capture_output=True
            )
            self.assertIn(
                "application/x-project-graph application/vnd.project-graph",
                (mime / "aliases").read_text(),
            )
            self.assertIn(
                "80:application/vnd.project-graph:*.prg", (mime / "globs2").read_text()
            )
            self.assertIn(
                "80:application/x-ravomi:*.mio", (mime / "globs2").read_text()
            )
            source, output = root / "中文 空格.prg", root / "preview.png"
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr(
                    "stage.json", json.dumps({"objects": [self.node("a", "中文预览")]})
                )
            subprocess.run(
                [str(helper), str(source), str(output), "128"],
                check=True,
                capture_output=True,
                timeout=20,
                env={**os.environ, "COLOR_SCHEME": "dark"},
            )
            surface = cairo.ImageSurface.create_from_png(str(output))
            self.assertEqual((surface.get_width(), surface.get_height()), (256, 256))

    def test_master_broken_reference_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "reference"):
            thumbnailer.legacy_graph(
                [{"_": "Section", "uuid": "a", "children": [{"$": "/missing"}]}]
            )

    def test_geometry_serializer_references_preserve_saved_node_bounds(self):
        stage = [
            {"_": "TextNode", "uuid": "a", "collisionBox": {"$": "/1"}},
            {"shapes": [{"$": "/2"}]},
            {"_": "Rectangle", "location": {"$": "/3"}, "size": {"x": 180, "y": 80}},
            {"x": -400, "y": 75},
        ]
        graph = thumbnailer.legacy_graph(stage)
        self.assertEqual(
            graph["objects"][0]["properties"]["preview_rect"], (-400, 75, 180, 80)
        )
        stage[3] = {"$": "/2/location"}
        with self.assertRaisesRegex(ValueError, "reference"):
            thumbnailer.legacy_graph(stage)

    def test_master_ports_use_exact_edges_and_connect_point_center(self):
        rect = (100, 200, 1000, 600)
        for rate, expected in [
            ((0.01, 0.99), (100, 500)),
            ((0.99, 0.01), (1100, 500)),
            ((0.5, 0.01), (600, 200)),
            ((0.5, 0.99), (600, 800)),
        ]:
            with self.subTest(rate=rate):
                self.assertEqual(
                    thumbnailer.master_port(rect, rate, (3000, 500))[0], expected
                )
        self.assertEqual(
            thumbnailer.master_port(rect, (0.5, 0.5), (3000, 500)),
            ((1100, 500), (1, 0)),
        )
        self.assertEqual(
            thumbnailer.master_port(rect, (0.99, 0.5), (3000, 500), point_node=True)[0],
            (600, 500),
        )

    def test_multi_target_edges_use_bounding_box_hub_and_one_caption(self):
        self.assertEqual(
            thumbnailer.multi_port((0, 0, 100, 100), (0.2, 0.8), (300, 80))[0],
            (100, 80),
        )
        self.assertEqual(
            thumbnailer.multi_port((0, 0, 100, 100), (0.01, 0.8), (300, 80))[0],
            (100, 80),
        )
        rectangles = {
            "a": (0, 0, 100, 40),
            "b": (400, 0, 100, 40),
            "c": (50, 300, 100, 40),
        }
        props = {
            "members": list(rectangles),
            "center_rate": (0.75, 0.5),
            "rect_rates": [],
            "arrow": "outer",
            "text": "中心",
        }
        edges = list(
            thumbnailer.edge_instances(
                [{"type": "multi_target_edge", "properties": props}], rectangles
            )
        )
        self.assertEqual(len(edges), 3)
        for index, (spoke, source, target, a, b) in enumerate(edges):
            self.assertEqual(source, "")
            self.assertEqual(a, (375, 170, 0, 0))
            self.assertEqual(b, rectangles[target])
            self.assertEqual(spoke["text"], "中心" if index == 0 else "")

    def test_master_line_and_arrow_styles_change_rendered_output(self):
        a, b = self.node("a", "起点", x=0, y=0), self.node("b", "终点", x=500, y=150)
        a["properties"].update(preview_kind="TextNode", font_size=24)
        props = {
            "preview_legacy": True,
            "source": {"$ref": "a"},
            "target": {"$ref": "b"},
            "source_rate": (0.99, 0.5),
            "target_rate": (0.01, 0.5),
            "text": "连线",
        }
        variants = [(line, "default") for line in ("solid", "dashed", "double")]
        variants += [
            ("solid", arrow)
            for arrow in (
                "none",
                "hollow-triangle",
                "filled-triangle",
                "hollow-diamond",
                "filled-diamond",
            )
        ]
        outputs = [
            self.render_graph(
                [
                    a,
                    b,
                    {
                        "type": "line_edge",
                        "properties": dict(props, line_type=line, arrow_type=arrow),
                    },
                ]
            )
            for line, arrow in variants
        ]
        self.assertEqual(len(set(outputs)), len(variants))

    def test_real_tutorial_renders_without_changing_source(self):
        source = (
            MODULE_PATH.parents[2] / "fixtures/tutorial-shortcut-keys-3.1-original.prg"
        )
        if not source.is_file():
            self.skipTest("Optional local tutorial fixture is not distributed")
        before = hashlib.sha256(source.read_bytes()).digest()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory, "tutorial.png")
            thumbnailer.render(str(source), str(output), 256)
            surface = cairo.ImageSurface.create_from_png(str(output))
            self.assertEqual((surface.get_width(), surface.get_height()), (512, 512))
            self.assertGreater(len(set(self.pixels(output.read_bytes()))), 100)
        self.assertEqual(hashlib.sha256(source.read_bytes()).digest(), before)

    def test_master_font_scale_controls_arrow_size_and_edge_caption(self):
        for level, arrow_size, caption_size in [(0, 16, 32), (2, 32, 64)]:
            with self.subTest(level=level):
                stage = [
                    {
                        "_": "TextNode",
                        "uuid": "a",
                        "text": "Source",
                        "fontScaleLevel": level,
                    },
                    {"_": "TextNode", "uuid": "b", "text": "Target"},
                    {
                        "_": "LineEdge",
                        "uuid": "edge",
                        "associationList": [{"$": "/0"}, {"$": "/1"}],
                        "sourceRectangleRate": {"x": 0.99, "y": 0.5},
                        "targetRectangleRate": {"x": 0.01, "y": 0.5},
                        "text": "Caption",
                    },
                ]
                objects = thumbnailer.legacy_graph(stage)["objects"]
                objects[1]["properties"]["preview_rect"] = (400, 0, 100, 60)
                with (
                    patch.object(
                        thumbnailer, "arrow_polygon", wraps=thumbnailer.arrow_polygon
                    ) as arrows,
                    patch.object(
                        thumbnailer, "layout_for", wraps=thumbnailer.layout_for
                    ) as layouts,
                ):
                    self.render_graph(objects)
                self.assertEqual(arrows.call_args.args[2], arrow_size)
                captions = [
                    call.args[2]
                    for call in layouts.call_args_list
                    if call.args[1] == "Caption"
                ]
                self.assertEqual(captions, [caption_size])

    def render_nested(self, directory, name, container_x):
        objects = []
        for identifier, parent, x, text in (
            ("outer", "", container_x, "Outer"),
            ("inner", "outer", container_x + 100, "Inner"),
            ("leaf", "inner", 200, "Project Graph"),
        ):
            props = {"id": identifier, "text": text, "font_size": 24}
            if parent:
                props["container"] = {"$ref": parent}
            objects.append(
                {
                    "type": "text_node",
                    "properties": props,
                    "transform": {"position": [x, 100]},
                }
            )
        # Cover connections to a container as well as the container itself.
        objects.append(
            {
                "type": "line_edge",
                "properties": {"source": {"$ref": "outer"}, "target": {"$ref": "leaf"}},
            }
        )
        source = Path(directory, name + ".prg")
        output = Path(directory, name + ".png")
        with zipfile.ZipFile(source, "w") as archive:
            archive.writestr("stage.json", json.dumps({"objects": objects}))
        with patch.object(thumbnailer, "prefers_light_theme", return_value=False):
            thumbnailer.render(str(source), str(output), 256)
        return output

    def test_hidden_container_positions_do_not_change_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            first = self.render_nested(directory, "near", 0)
            second = self.render_nested(directory, "far", -2000)
            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_preview_has_two_pixels_per_requested_pixel(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.render_nested(directory, "resolution", 0)
            surface = cairo.ImageSurface.create_from_png(str(output))
            self.assertEqual((surface.get_width(), surface.get_height()), (512, 512))


if __name__ == "__main__":
    unittest.main()
