//! UI-independent command surface for the React/Tauri migration prototype.
//! Document remains the sole owner of edits, history and PRG persistence.
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    New { discard: bool },
    Open { path: String, discard: bool },
    Save { path: String },
    AddText { x: f32, y: f32 },
    EditText { id: String, text: String },
    Undo,
    Redo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub project_id: String,
    pub document_token: String,
    pub archived_workflow_count: usize,
    pub title: String,
    pub path: Option<String>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub nodes: Vec<Node>,
    pub edge_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub rect: [f32; 4],
    pub editable: bool,
}

pub struct Session {
    document: Document,
    document_token: String,
}

impl Session {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            document: Document::blank().map_err(|error| error.to_string())?,
            document_token: uuid::Uuid::new_v4().to_string(),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let document = &self.document;
        let protected = document.protected_nodes();
        Snapshot {
            project_id: document.project_id().to_owned(),
            document_token: self.document_token.clone(),
            archived_workflow_count: document.history_archive().workflows.len(),
            title: document
                .path
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未命名画布".into()),
            path: document
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            dirty: document.dirty(),
            can_undo: document.history.can_undo(),
            can_redo: document.history.can_redo(),
            nodes: document
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| Node {
                    id: node.id.clone(),
                    kind: node.kind.clone(),
                    text: node.text.clone(),
                    rect: node.rect,
                    editable: node.kind == "TextNode" && !protected.contains(&index),
                })
                .collect(),
            edge_count: document.edges.len(),
        }
    }

    pub fn project_id(&self) -> &str {
        self.document.project_id()
    }

    /// A local handle for this open document, distinct from the portable ID
    /// shared by Save As copies. It is never serialized into the project file.
    pub fn document_token(&self) -> &str {
        &self.document_token
    }

    pub fn archived_history(&self) -> &crate::project_history::History {
        self.document.history_archive()
    }

    pub fn attach_history(
        &mut self,
        history: crate::project_history::History,
    ) -> Result<Snapshot, String> {
        self.document
            .attach_history(history)
            .map_err(|error| error.to_string())?;
        Ok(self.snapshot())
    }

    pub fn execute(&mut self, command: Command) -> Result<Snapshot, String> {
        match command {
            Command::New { discard } => {
                self.check_discard(discard)?;
                self.document = Document::blank().map_err(|error| error.to_string())?;
                self.document_token = uuid::Uuid::new_v4().to_string();
            }
            Command::Open { path, discard } => {
                self.check_discard(discard)?;
                // Parse before replacing the live document; a failure preserves edits.
                self.document =
                    Document::open(Path::new(&path)).map_err(|error| error.to_string())?;
                self.document_token = uuid::Uuid::new_v4().to_string();
            }
            Command::Save { path } => self
                .document
                .save(Path::new(&path))
                .map_err(|error| error.to_string())?,
            Command::AddText { x, y } => {
                if ![x, y]
                    .iter()
                    .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
                {
                    return Err("节点位置超出有效范围".into());
                }
                self.document
                    .create_text_node([x, y])
                    .map_err(|error| error.to_string())?;
            }
            Command::EditText { id, text } => {
                let index = self
                    .document
                    .nodes
                    .iter()
                    .position(|node| node.id == id)
                    .ok_or("节点已不存在，请重新选择")?;
                if self.document.nodes[index].kind != "TextNode"
                    || !self.document.can_edit_text(index)
                {
                    return Err("此节点不能在当前界面编辑".into());
                }
                if text.len() > 1024 * 1024 {
                    return Err("节点文字不能超过 1 MiB".into());
                }
                let rect = self.document.nodes[index].rect;
                if !rect[2..]
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
                {
                    return Err("节点尺寸无效，请在原画布中修复后再编辑".into());
                }
                // Preserve document geometry until the new canvas has a layout engine.
                self.document.edit_text(&id, text, [rect[2], rect[3]]);
            }
            Command::Undo => self.document.undo(),
            Command::Redo => self.document.redo(),
        }
        Ok(self.snapshot())
    }

    fn check_discard(&self, discard: bool) -> Result<(), String> {
        if self.document.dirty() && !discard {
            Err("当前画布尚未保存，请保存或确认放弃更改".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_through_existing_prg_and_history() {
        let mut session = Session::new().unwrap();
        let snapshot = session
            .execute(Command::AddText { x: 0.0, y: 0.0 })
            .unwrap();
        let id = snapshot.nodes[0].id.clone();
        let before = snapshot.nodes[0].text.clone();
        session
            .execute(Command::EditText {
                id,
                text: "你好，Ravomi".into(),
            })
            .unwrap();
        assert_eq!(
            session.execute(Command::Undo).unwrap().nodes[0].text,
            before
        );
        assert_eq!(
            session.execute(Command::Redo).unwrap().nodes[0].text,
            "你好，Ravomi"
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("中文.prg")
            .to_string_lossy()
            .into_owned();
        assert!(
            !session
                .execute(Command::Save { path: path.clone() })
                .unwrap()
                .dirty
        );
        session.execute(Command::New { discard: false }).unwrap();
        let loaded = session
            .execute(Command::Open {
                path,
                discard: false,
            })
            .unwrap();
        assert_eq!(loaded.nodes[0].text, "你好，Ravomi");
        assert!(!loaded.dirty);
    }

    #[test]
    fn rejected_commands_preserve_the_current_document() {
        let mut session = Session::new().unwrap();
        session
            .execute(Command::AddText { x: 10.0, y: 20.0 })
            .unwrap();
        assert!(session.execute(Command::New { discard: false }).is_err());
        let directory = tempfile::tempdir().unwrap();
        assert!(
            session
                .execute(Command::Open {
                    path: directory
                        .path()
                        .join("missing.prg")
                        .to_string_lossy()
                        .into_owned(),
                    discard: true,
                })
                .is_err()
        );
        assert!(
            session
                .execute(Command::AddText {
                    x: f32::NAN,
                    y: 0.0
                })
                .is_err()
        );
        let id = session.document.nodes[0].id.clone();
        session.document.nodes[0].locked = true;
        assert!(
            session
                .execute(Command::EditText {
                    id,
                    text: "不能写入".into()
                })
                .is_err()
        );
        assert_eq!(session.snapshot().nodes.len(), 1);
        assert!(session.snapshot().dirty);
    }
}
