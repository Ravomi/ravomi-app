use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fs::File, io::Read, path::Path};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub mod collaboration;
pub mod document;
pub mod edit_protocol;
pub mod project;
pub mod project_history;
pub mod recovery;
pub mod session;
pub mod storage;

pub struct ProjectFile {
    pub stage: Value,
    pub attachments: BTreeMap<String, Vec<u8>>,
}

impl ProjectFile {
    pub fn load(path: &Path) -> Result<Self> {
        Self::read(File::open(path)?)
    }

    /// Decode the same snapshot that the document retains for lossless saving
    /// and external-change detection, without reopening a mutable disk path.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::read(std::io::Cursor::new(bytes))
    }

    fn read(reader: impl Read + std::io::Seek) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let contents = project::inspect(&mut archive)?;
        let stage_bytes =
            project::read_entry(&mut archive, contents.format.stage(), 128 * 1024 * 1024)?;
        let stage: Value = rmp_serde::from_slice(&stage_bytes)?;
        if !stage.is_array() {
            return Err("stage.msgpack must contain an array".into());
        }
        let mut attachments = BTreeMap::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            if let Some(name) = entry.name().strip_prefix(contents.format.assets()) {
                let name = name.to_owned();
                if entry.size() > 64 * 1024 * 1024 {
                    return Err("Attachment exceeds 64 MiB".into());
                }
                let mut bytes = Vec::new();
                entry.take(64 * 1024 * 1024).read_to_end(&mut bytes)?;
                attachments.insert(name, bytes);
            }
        }
        Ok(Self { stage, attachments })
    }

    pub fn resolve<'a>(&'a self, value: &'a Value) -> Result<&'a Value> {
        if let Some(path) = value.get("$").and_then(Value::as_str) {
            self.stage
                .pointer(path)
                .ok_or_else(|| format!("Missing reference: {path}").into())
        } else {
            Ok(value)
        }
    }

    pub fn objects(&self) -> BTreeMap<String, &Value> {
        fn visit<'a>(value: &'a Value, objects: &mut BTreeMap<String, &'a Value>) {
            match value {
                Value::Object(map) => {
                    if let Some(id) = map.get("uuid").and_then(Value::as_str) {
                        objects.insert(id.to_owned(), value);
                    }
                    for child in map.values() {
                        visit(child, objects);
                    }
                }
                Value::Array(values) => {
                    for child in values {
                        visit(child, objects);
                    }
                }
                _ => {}
            }
        }
        let mut objects = BTreeMap::new();
        visit(&self.stage, &mut objects);
        objects
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub rect: [f32; 4],
    pub text: String,
    /// Destination of a URL node, kept separate from its display title.
    pub url: Option<String>,
    pub font_size: f32,
    pub auto_size: bool,
    pub color: [f32; 4],
    pub attachment: Option<String>,
    pub source_name: Option<String>,
    pub children: Vec<String>,
    pub locked: bool,
    pub collapsed: bool,
    /// Coordinates relative to the node rectangle, followed by pen pressure.
    pub stroke: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphEdge {
    pub id: String,
    pub members: Vec<usize>,
    pub rates: Vec<[f32; 2]>,
    pub text: String,
    pub color: [f32; 4],
    pub arrow: bool,
    pub kind: String,
    pub line_type: String,
    pub arrow_type: String,
    pub render_type: String,
    pub multi_arrow: String,
    pub center_rate: [f32; 2],
    pub padding: f32,
    pub arc_offset: f32,
    pub text_position: f32,
}
impl Default for GraphEdge {
    fn default() -> Self {
        Self {
            id: String::new(),
            members: vec![],
            rates: vec![],
            text: String::new(),
            color: [0.0; 4],
            arrow: true,
            kind: "LineEdge".into(),
            line_type: "solid".into(),
            arrow_type: "default".into(),
            render_type: "line".into(),
            multi_arrow: "none".into(),
            center_rate: [0.5; 2],
            padding: 10.0,
            arc_offset: 0.0,
            text_position: 0.5,
        }
    }
}

pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub attachments: BTreeMap<String, Vec<u8>>,
    pub counts: BTreeMap<String, usize>,
}

fn number(value: &Value, fallback: f32) -> f32 {
    value
        .as_f64()
        .map(|n| n as f32)
        .filter(|n| n.is_finite())
        .unwrap_or(fallback)
}
fn vector(value: &Value) -> [f32; 2] {
    [number(&value["x"], 0.0), number(&value["y"], 0.0)]
}
fn color(value: &Value) -> [f32; 4] {
    [
        number(&value["r"], 0.0) / 255.0,
        number(&value["g"], 0.0) / 255.0,
        number(&value["b"], 0.0) / 255.0,
        number(&value["a"], 0.0),
    ]
}

fn bounds(value: &Value) -> Result<[f32; 4]> {
    let shapes = value
        .get("shapes")
        .and_then(Value::as_array)
        .ok_or("Missing collision shapes")?;
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    for shape in shapes {
        let (a, b) = match shape["_"].as_str() {
            Some("Rectangle") => {
                let p = vector(&shape["location"]);
                let s = vector(&shape["size"]);
                (p, [p[0] + s[0], p[1] + s[1]])
            }
            Some("Line") => (vector(&shape["start"]), vector(&shape["end"])),
            Some("Circle") => {
                let p = vector(&shape["location"]);
                let r = number(&shape["radius"], 0.0);
                ([p[0] - r, p[1] - r], [p[0] + r, p[1] + r])
            }
            _ => return Err(format!("Unsupported collision shape: {}", shape["_"]).into()),
        };
        for axis in 0..2 {
            lo[axis] = lo[axis].min(a[axis]).min(b[axis]);
            hi[axis] = hi[axis].max(a[axis]).max(b[axis]);
        }
    }
    if !lo[0].is_finite() {
        return Err("Empty collision box".into());
    }
    Ok([
        lo[0],
        lo[1],
        (hi[0] - lo[0]).max(1.0),
        (hi[1] - lo[1]).max(1.0),
    ])
}

impl Graph {
    pub fn export_scene(&self, path: &Path) -> Result<()> {
        let nodes: Vec<_> = self.nodes.iter().map(|n| serde_json::json!({"id":n.id,"kind":n.kind,"rect":n.rect,"text":n.text,"font_size":n.font_size,"color":n.color})).collect();
        let edges: Vec<_> = self.edges.iter().map(|e| serde_json::json!({"members":e.members,"rates":e.rates,"text":e.text,"color":e.color})).collect();
        std::fs::write(
            path,
            serde_json::to_vec(
                &serde_json::json!({"nodes":nodes,"edges":edges,"bounds":self.bounds()}),
            )?,
        )?;
        Ok(())
    }

    pub fn from_file(file: ProjectFile) -> Result<Self> {
        let objects = file.objects();
        let mut nodes = Vec::new();
        let mut counts = BTreeMap::new();
        for (id, object) in &objects {
            let kind = object["_"].as_str().ok_or("Object missing type")?;
            *counts.entry(kind.to_owned()).or_insert(0) += 1;
            if kind.ends_with("Edge") || kind == "SyncAssociation" {
                continue;
            }
            if ![
                "TextNode",
                "Section",
                "ConnectPoint",
                "ImageNode",
                "SvgNode",
                "UrlNode",
                "LatexNode",
                "PenStroke",
            ]
            .contains(&kind)
            {
                return Err(format!("Unsupported node type: {kind}").into());
            }
            let mut stroke = Vec::new();
            let rect = if kind == "PenStroke" {
                let segments = object["segments"]
                    .as_array()
                    .ok_or("PenStroke missing segments")?;
                let mut lo = [f32::INFINITY; 2];
                let mut hi = [f32::NEG_INFINITY; 2];
                for segment in segments {
                    let segment = file.resolve(segment)?;
                    let location = file.resolve(&segment["location"])?;
                    let coordinate = |axis: &str| -> Result<f32> {
                        let v = location[axis].as_f64().ok_or("Invalid pen coordinate")? as f32;
                        if !v.is_finite() {
                            return Err("Non-finite pen coordinate".into());
                        }
                        Ok(v)
                    };
                    let pressure = number(&segment["pressure"], 1.0);
                    if !pressure.is_finite() || pressure < 0.0 {
                        return Err("Invalid pen pressure".into());
                    }
                    let p = [coordinate("x")?, coordinate("y")?, pressure];
                    for axis in 0..2 {
                        lo[axis] = lo[axis].min(p[axis]);
                        hi[axis] = hi[axis].max(p[axis]);
                    }
                    stroke.push(p);
                }
                if stroke.is_empty() {
                    return Err("Empty PenStroke".into());
                }
                let rect = [
                    lo[0],
                    lo[1],
                    (hi[0] - lo[0]).max(1.0),
                    (hi[1] - lo[1]).max(1.0),
                ];
                for p in &mut stroke {
                    for axis in 0..2 {
                        p[axis] = (p[axis] - rect[axis]) / rect[axis + 2];
                    }
                }
                rect
            } else {
                let collision = object
                    .get("collisionBox")
                    .or_else(|| object.get("_collisionBoxNormal"))
                    .ok_or("Node missing collision box")?;
                bounds(file.resolve(collision)?)?
            };
            let children = object["children"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .map(|v| {
                            file.resolve(v)?
                                .get("uuid")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .ok_or_else(|| "Child missing uuid".into())
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default();
            let attachment_id = object["attachmentId"].as_str();
            let attachment = attachment_id.and_then(|id| {
                file.attachments
                    .keys()
                    .find(|name| name.starts_with(&format!("{id}.")))
                    .cloned()
            });
            if attachment_id.is_some_and(|id| !id.is_empty()) && attachment.is_none() {
                return Err(format!("Missing attachment for {id}").into());
            }
            nodes.push(GraphNode {
                id: id.clone(),
                kind: kind.to_owned(),
                rect,
                stroke,
                text: object[if kind == "UrlNode" { "title" } else { "text" }]
                    .as_str()
                    .or_else(|| object["text"].as_str())
                    .or_else(|| object["title"].as_str())
                    .unwrap_or("")
                    .to_owned(),
                url: object["url"].as_str().map(str::to_owned),
                font_size: {
                    let size = 32.0 * 2.0_f32.powf(number(&object["fontScaleLevel"], 0.0) / 2.0);
                    number(
                        &object["fontSize"],
                        if size >= 2.0 { size.floor() } else { size },
                    )
                },
                color: color(&object["color"]),
                auto_size: kind == "TextNode" && object["sizeAdjust"].as_str() != Some("manual"),
                attachment,
                source_name: object["grapluSourceName"].as_str().map(str::to_owned),
                children,
                locked: object["locked"].as_bool().unwrap_or(false),
                collapsed: kind == "Section" && object["isCollapsed"].as_bool().unwrap_or(false),
            });
        }
        let indices: BTreeMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut edges = Vec::new();
        for (id, object) in &objects {
            let kind = object["_"].as_str().unwrap();
            if kind == "SyncAssociation" || !kind.ends_with("Edge") {
                continue;
            }
            if !["LineEdge", "ArcEdge", "MultiTargetUndirectedEdge"].contains(&kind) {
                return Err(format!("Unsupported edge type: {kind}").into());
            }
            let members = object["associationList"]
                .as_array()
                .ok_or("Edge missing members")?
                .iter()
                .map(|v| {
                    let member = file.resolve(v)?;
                    let uuid = member["uuid"].as_str().ok_or("Endpoint missing uuid")?;
                    indices
                        .get(uuid)
                        .copied()
                        .ok_or_else(|| format!("Unresolved endpoint: {uuid}").into())
                })
                .collect::<Result<Vec<_>>>()?;
            if members.len() < 2 || (kind != "MultiTargetUndirectedEdge" && members.len() != 2) {
                return Err("Edge has fewer than two endpoints".into());
            }
            let read_rate = |value: Option<&Value>| -> Result<[f32; 2]> {
                value
                    .map(|v| file.resolve(v).map(vector))
                    .unwrap_or(Ok([0.5; 2]))
            };
            let rates = if kind != "MultiTargetUndirectedEdge" {
                vec![
                    read_rate(object.get("sourceRectangleRate"))?,
                    read_rate(object.get("targetRectangleRate"))?,
                ]
            } else {
                (0..members.len())
                    .map(|i| read_rate(object["rectRates"].get(i)))
                    .collect::<Result<Vec<_>>>()?
            };
            edges.push(GraphEdge {
                id: id.clone(),
                members,
                rates,
                text: object["text"].as_str().unwrap_or("").to_owned(),
                color: color(&object["color"]),
                arrow: kind != "MultiTargetUndirectedEdge"
                    && object["arrowType"].as_str() != Some("none"),
                kind: kind.to_owned(),
                line_type: object["lineType"].as_str().unwrap_or("solid").into(),
                arrow_type: object["arrowType"].as_str().unwrap_or("default").into(),
                render_type: object["renderType"].as_str().unwrap_or("line").into(),
                multi_arrow: object["arrow"].as_str().unwrap_or("none").into(),
                center_rate: read_rate(object.get("centerRate"))?,
                padding: number(&object["padding"], 10.0).max(0.0),
                arc_offset: number(&object["offset"], 0.0),
                text_position: number(&object["textPosition"], 0.5),
            });
        }
        drop(objects);
        Ok(Self {
            nodes,
            edges,
            attachments: file.attachments,
            counts,
        })
    }

    pub fn bounds(&self) -> [f32; 4] {
        if self.nodes.is_empty() {
            return [-100.0, -100.0, 200.0, 200.0];
        }
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for node in &self.nodes {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(node.rect[axis]);
                hi[axis] = hi[axis].max(node.rect[axis] + node.rect[axis + 2]);
            }
        }
        [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "Download the pinned original tutorial with run.sh first"]
    fn original_tutorial_import_is_complete() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/tutorial-shortcut-keys-3.1-original.prg");
        let file = ProjectFile::load(&path).unwrap();
        assert_eq!(file.stage.as_array().unwrap().len(), 623);
        let graph = Graph::from_file(file).unwrap();
        assert_eq!(graph.nodes.len(), 393);
        assert_eq!(graph.edges.len(), 230);
        assert_eq!(graph.attachments.len(), 14);
        assert_eq!(graph.counts["TextNode"], 345);
        assert_eq!(graph.counts["Section"], 27);
        assert_eq!(graph.counts["ConnectPoint"], 21);
        assert_eq!(
            graph
                .nodes
                .iter()
                .find(|n| n.text == "全局快捷键")
                .unwrap()
                .font_size,
            64.0
        );
        assert!(
            graph
                .edges
                .iter()
                .all(|edge| edge.members.iter().all(|index| *index < graph.nodes.len()))
        );
    }

    #[test]
    fn collects_nested_entities_once_and_resolves_edge_paths() {
        let node = json!({"_":"TextNode","uuid":"a","text":"中文","collisionBox":{"shapes":[{"_":"Rectangle","location":{"x":10,"y":20},"size":{"x":30,"y":40}}]}});
        let file = ProjectFile {
            stage: json!([node, {"$":"/0"}, {"_":"LineEdge","uuid":"edge","associationList":[{"$":"/0"},{"$":"/0"}],"sourceRectangleRate":{"x":0.01,"y":0.5},"targetRectangleRate":{"$":"/2/sourceRectangleRate"},"lineType":"dashed","arrowType":"hollow-triangle"}, {"_":"MultiTargetUndirectedEdge","uuid":"multi","associationList":[{"$":"/0"},{"$":"/0"}],"rectRates":[{"$":"/2/sourceRectangleRate"},{"x":0.8,"y":0.3}],"centerRate":{"$":"/3/rectRates/1"}}]),
            attachments: BTreeMap::new(),
        };
        let graph = Graph::from_file(file).unwrap();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].text, "中文");
        assert_eq!(graph.edges[0].members, [0, 0]);
        assert_eq!(graph.edges[0].rates, [[0.01, 0.5]; 2]);
        assert_eq!(graph.edges[0].line_type, "dashed");
        assert_eq!(graph.edges[0].arrow_type, "hollow-triangle");
        assert_eq!(graph.edges[1].rates, [[0.01, 0.5], [0.8, 0.3]]);
        assert_eq!(graph.edges[1].center_rate, [0.8, 0.3]);
        assert_eq!(graph.bounds(), [10.0, 20.0, 30.0, 40.0]);
    }

    #[test]
    fn missing_reference_is_an_error() {
        let file = ProjectFile {
            stage: json!([]),
            attachments: BTreeMap::new(),
        };
        assert!(file.resolve(&json!({"$":"/99"})).is_err());
    }

    #[test]
    fn text_layout_modes_default_to_auto_and_do_not_apply_to_other_nodes() {
        for (kind, mode, auto_size) in [
            ("TextNode", None, true),
            ("TextNode", Some("auto"), true),
            ("TextNode", Some("manual"), false),
            ("Section", Some("auto"), false),
        ] {
            let mut node = json!({"_":kind,"uuid":"node","text":"中文","collisionBox":{"shapes":[{"_":"Rectangle","location":{"x":0,"y":0},"size":{"x":100,"y":80}}]}});
            if let Some(mode) = mode {
                node["sizeAdjust"] = mode.into();
            }
            let graph = Graph::from_file(ProjectFile {
                stage: json!([node]),
                attachments: BTreeMap::new(),
            })
            .unwrap();
            assert_eq!(graph.nodes[0].auto_size, auto_size);
        }
    }

    #[test]
    fn section_bounds_include_lines_and_title_rectangle() {
        assert_eq!(bounds(&json!({"shapes":[{"_":"Line","start":{"x":10,"y":20},"end":{"x":40,"y":20}}, {"_":"Rectangle","location":{"x":10,"y":20},"size":{"x":30,"y":50}}]})).unwrap(), [10.0,20.0,30.0,50.0]);
    }
}
