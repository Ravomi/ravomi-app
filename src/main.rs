use project_graph_bevy_canvas::{Graph, ProjectFile, Result};
use std::path::Path;

#[cfg(feature = "viewer")]
mod viewer;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(feature = "viewer")]
    if args.first().is_none_or(|arg| arg.starts_with("--")) {
        let mut args = args;
        args.push("--home".into());
        return viewer::run_home(&args);
    }
    let path = args.first().ok_or("Usage: project-graph-bevy-canvas FILE.mio|FILE.prg [--inspect] [--benchmark] [--font FONT.ttf] [--output results.json]")?;
    let file = ProjectFile::load(Path::new(&path))?;
    let graph = Graph::from_file(file)?;
    println!(
        "{:?}; nodes={}; edges={}; attachments={}; bounds={:?}",
        graph.counts,
        graph.nodes.len(),
        graph.edges.len(),
        graph.attachments.len(),
        graph.bounds()
    );
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--export") {
        graph.export_scene(Path::new(&pair[1]))?;
        return Ok(());
    }
    if args.iter().any(|a| a == "--inspect") {
        return Ok(());
    }
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--save-copy") {
        project_graph_bevy_canvas::document::Document::open(Path::new(path))?
            .save(Path::new(&pair[1]))?;
        return Ok(());
    }
    #[cfg(feature = "viewer")]
    viewer::run(
        project_graph_bevy_canvas::document::Document::open(Path::new(path))?,
        &args,
    )?;
    Ok(())
}
