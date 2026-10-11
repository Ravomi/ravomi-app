use project_graph_bevy_canvas::{ProjectFile, Result};
use std::{collections::BTreeMap, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage: inspect_prg FILE.prg [TEXT_OUTPUT.txt]")?;
    let file = ProjectFile::load(Path::new(path))?;
    let mut counts = BTreeMap::new();
    let mut text = String::new();
    for object in file.objects().values() {
        let kind = object["_"].as_str().unwrap_or("unknown");
        *counts.entry(kind).or_insert(0usize) += 1;
        for field in ["text", "title"] {
            if let Some(value) = object[field].as_str() {
                text.push_str(value);
                text.push('\n');
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"counts":counts,"attachments":file.attachments.len()})
    );
    if let Some(output) = args.get(1) {
        std::fs::write(output, text)?;
    }
    Ok(())
}
