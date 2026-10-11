//! Text-input messages shared by a native canvas host and a WebView frontend.
//! Preedit is transient: only Commit may become a document operation.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Ready,
    Draft {
        session: u64,
        text: String,
        composing: bool,
        width: f32,
        height: f32,
    },
    Commit {
        session: u64,
        text: String,
        width: f32,
        height: f32,
    },
    Cancel {
        session: u64,
    },
}

pub fn decode(json: &str) -> Result<Input, String> {
    if json.len() > 2 * 1024 * 1024 {
        return Err("编辑消息超过大小限制".into());
    }
    let input: Input = serde_json::from_str(json).map_err(|e| format!("编辑消息无效：{e}"))?;
    match &input {
        Input::Draft {
            text,
            width,
            height,
            ..
        }
        | Input::Commit {
            text,
            width,
            height,
            ..
        } => {
            if text.len() > 1024 * 1024 {
                return Err("节点文字不能超过 1 MiB".into());
            }
            if ![width, height]
                .into_iter()
                .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1_000_000.0)
            {
                return Err("编辑预览尺寸无效".into());
            }
        }
        _ => {}
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_and_commit_are_distinct_and_geometry_is_bounded() {
        assert!(matches!(decode(r#"{"type":"draft","session":4,"text":"ni hao","composing":true,"width":200,"height":48}"#).unwrap(), Input::Draft { composing: true, .. }));
        assert!(matches!(
            decode(r#"{"type":"commit","session":4,"text":"你好","width":64,"height":48}"#)
                .unwrap(),
            Input::Commit { .. }
        ));
        for width in ["-1", "1000001", "null"] {
            assert!(
                decode(&format!(
                    r#"{{"type":"commit","session":4,"text":"a","width":{width},"height":48}}"#
                ))
                .is_err()
            );
        }
        assert!(decode(r#"{"type":"cancel","session":4,"text":"unexpected"}"#).is_err());
    }
}
