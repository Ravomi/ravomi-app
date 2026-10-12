use super::*;
use std::{
    io::Read,
    sync::{Mutex, mpsc},
};

pub struct ImportedImage {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub pixels: (u32, u32, Vec<u8>),
}

type ImportResult = std::result::Result<Option<ImportedImage>, String>;

#[allow(clippy::too_many_arguments)]
pub fn dropped_files(
    mut events: MessageReader<bevy::window::FileDragAndDrop>,
    mut queued: Local<std::collections::VecDeque<PathBuf>>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
    config: Res<Config>,
    mut editor: ResMut<editor::Editor>,
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    mut physics: ResMut<physics::PhysicsEditor>,
) {
    for event in events.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile {
            window: target,
            path_buf,
        } = event
            && *target == window.0
        {
            queued.push_back(path_buf.clone());
        }
    }
    if editor.tabs_blocked() || editor.active.is_some() {
        return;
    }
    let Some(path) = queued.pop_front() else {
        return;
    };
    let (transform, Projection::Orthographic(projection)) = *camera else {
        return;
    };
    let point = window
        .1
        .cursor_position()
        .map(|cursor| world_cursor(cursor, window.1, transform, projection.scale))
        .unwrap_or(transform.translation.truncate());
    physics.finish(&mut scene.0);
    interaction.creation.cancel();
    interaction.cutting.cancel();
    interaction.jump.stop(&mut scene.0);
    interaction.moving.clear();
    interaction.linking = None;
    scene.0.selection = project_graph_bevy_canvas::document::Selection::capture(
        &scene.0,
        &interaction.selected,
        &interaction.selected_edges,
    );
    editor.importing = Some(Pending::file(
        path,
        [point.x, -point.y],
        512.0 * projection.scale,
        config.font.clone(),
    ));
}

pub struct Pending {
    receiver: Mutex<mpsc::Receiver<ImportResult>>,
    pub center: [f32; 2],
    pub max_size: f32,
    pub paste_graph: bool,
}

impl Pending {
    pub fn start(center: [f32; 2], max_size: f32, font: Vec<u8>) -> Self {
        Self::spawn(center, max_size, move || {
            let Some(path) = rfd::FileDialog::new()
                .set_title("导入图片")
                .add_filter("图片", &["png", "jpg", "jpeg", "svg"])
                .pick_file()
            else {
                return Ok(None);
            };
            ImportedImage::from_path(&path, &font).map(Some)
        })
    }

    pub fn file(path: PathBuf, center: [f32; 2], max_size: f32, font: Vec<u8>) -> Self {
        Self::spawn(center, max_size, move || {
            ImportedImage::from_path(&path, &font).map(Some)
        })
    }

    pub fn paste(center: [f32; 2], max_size: f32, font: Vec<u8>) -> Self {
        let mut pending = Self::spawn(center, max_size, move || {
            let mut clipboard = arboard::Clipboard::new()?;
            match clipboard.get_text() {
                Ok(text) => {
                    if let Some(image) = ImportedImage::from_svg_text(&text, &font)? {
                        return Ok(Some(image));
                    }
                }
                Err(arboard::Error::ContentNotAvailable) => {}
                Err(error) => return Err(format!("无法读取剪贴板：{error}").into()),
            }
            match clipboard.get_image() {
                Ok(image) => ImportedImage::from_clipboard(image).map(Some),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(error) => Err(format!("无法读取剪贴板图片：{error}").into()),
            }
        });
        pending.paste_graph = true;
        pending
    }

    pub fn clipboard(center: [f32; 2], max_size: f32) -> Self {
        Self::spawn(center, max_size, || {
            let mut clipboard = arboard::Clipboard::new()?;
            let image = clipboard.get_image().map_err(|error| match error {
                arboard::Error::ContentNotAvailable => {
                    "剪贴板中没有可读取的图片，请先复制图片再重试".to_owned()
                }
                error => format!("无法读取剪贴板图片：{error}"),
            })?;
            Ok(Some(ImportedImage::from_clipboard(image)?))
        })
    }

    fn spawn(
        center: [f32; 2],
        max_size: f32,
        task: impl FnOnce() -> Result<Option<ImportedImage>> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = task().map_err(|error| error.to_string());
            // Closing the app may drop the receiver while the native dialog is open.
            let _ = sender.send(result);
        });
        Self {
            receiver: Mutex::new(receiver),
            center,
            max_size,
            paste_graph: false,
        }
    }

    pub fn poll(&self) -> Option<ImportResult> {
        let receiver = match self.receiver.lock() {
            Ok(receiver) => receiver,
            Err(_) => return Some(Err("图片导入任务无法读取，请重试".into())),
        };
        match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("图片导入任务意外结束，请重试".into()))
            }
        }
    }
}

impl ImportedImage {
    fn from_path(path: &std::path::Path, font: &[u8]) -> Result<Self> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("图片文件名不是有效的 Unicode 文本")?
            .to_owned();
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("图片文件超过 64 MiB，请缩小后重试".into());
        }
        let pixels = attachment_pixels(&filename, &bytes, &mut None, font)?;
        Ok(Self {
            filename,
            bytes,
            pixels,
        })
    }

    fn from_svg_text(text: &str, font: &[u8]) -> Result<Option<Self>> {
        let source = text.trim_start_matches('\u{feff}').trim();
        if !(source.starts_with("<svg")
            || ((source.starts_with("<?xml") || source.starts_with("<!--"))
                && source.contains("<svg")))
        {
            return Ok(None);
        }
        if source.len() > 64 * 1024 * 1024 {
            return Err("SVG 源码超过 64 MiB，请缩小后重试".into());
        }
        let bytes = source.as_bytes().to_vec();
        let pixels = attachment_pixels("clipboard.svg", &bytes, &mut None, font)?;
        Ok(Some(Self {
            filename: "clipboard.svg".into(),
            bytes,
            pixels,
        }))
    }

    fn from_clipboard(data: arboard::ImageData<'_>) -> Result<Self> {
        let width = u32::try_from(data.width)?;
        let height = u32::try_from(data.height)?;
        check_attachment_size(width, height)?;
        if data.bytes.len() != width as usize * height as usize * 4 {
            return Err("剪贴板图片的像素数据不完整".into());
        }
        let image = image::RgbaImage::from_raw(width, height, data.bytes.into_owned())
            .ok_or("剪贴板图片的尺寸与像素不匹配")?;
        let mut encoded = std::io::Cursor::new(Vec::new());
        image.write_to(&mut encoded, image::ImageFormat::Png)?;
        Ok(Self {
            filename: "clipboard.png".into(),
            bytes: encoded.into_inner(),
            pixels: (width, height, image.into_raw()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn svg_source_paste_keeps_vector_attachment_and_history() {
        let source = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\"><rect width=\"40\" height=\"20\" fill=\"red\"/></svg>";
        let imported = ImportedImage::from_svg_text(source, &[]).unwrap().unwrap();
        assert_eq!((imported.pixels.0, imported.pixels.1), (40, 20));
        let mut doc = Document::blank().unwrap();
        let index = doc
            .import_image([0.0; 2], [40.0, 20.0], &imported.filename, imported.bytes)
            .unwrap();
        assert_eq!(doc.nodes[index].kind, "SvgNode");
        let attachment = doc.nodes[index].attachment.clone().unwrap();
        doc.undo();
        assert!(doc.nodes.is_empty());
        doc.redo();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("svg.mio");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert_eq!(reopened.attachments[&attachment], source.as_bytes());
        assert!(
            ImportedImage::from_svg_text("ordinary text", &[])
                .unwrap()
                .is_none()
        );
        assert!(ImportedImage::from_svg_text("<svg invalid", &[]).is_err());
        assert!(
            ImportedImage::from_svg_text(&format!("\u{feff} <?xml version=\"1.0\"?>{source}"), &[])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn dropped_image_file_uses_the_validated_import_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drop.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 40, 60, 128]))
            .save(&path)
            .unwrap();
        let imported = ImportedImage::from_path(&path, &[]).unwrap();
        assert_eq!(imported.filename, "drop.png");
        assert_eq!((imported.pixels.0, imported.pixels.1), (3, 2));
        assert_eq!(&imported.pixels.2[..4], &[20, 40, 60, 128]);
        std::fs::write(&path, "not an image").unwrap();
        assert!(ImportedImage::from_path(&path, &[]).is_err());
    }

    #[test]
    fn clipboard_pixels_keep_alpha_in_saved_png_and_reject_invalid_buffers() {
        let rgba = [24, 80, 220, 128].repeat(4);
        let imported = ImportedImage::from_clipboard(arboard::ImageData {
            width: 2,
            height: 2,
            bytes: Cow::Borrowed(&rgba),
        })
        .unwrap();
        let decoded = image::load_from_memory(&imported.bytes)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.as_raw(), &rgba);
        assert_eq!(imported.pixels, (2, 2, rgba));
        for (width, height, bytes) in [(0, 1, vec![]), (2, 2, vec![0; 15]), (8193, 1, vec![])] {
            assert!(
                ImportedImage::from_clipboard(arboard::ImageData {
                    width,
                    height,
                    bytes: Cow::Owned(bytes),
                })
                .is_err()
            );
        }
    }

    #[test]
    #[ignore = "requires an isolated X11 clipboard and GRAPLU_ISOLATED_CLIPBOARD=1"]
    fn system_clipboard_image_is_imported_and_survives_save_reopen() {
        assert_eq!(
            std::env::var("GRAPLU_ISOLATED_CLIPBOARD").as_deref(),
            Ok("1")
        );
        assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
        let mut owner = arboard::Clipboard::new().unwrap();
        let rgba = [24, 80, 220, 128].repeat(4);
        owner
            .set_image(arboard::ImageData {
                width: 2,
                height: 2,
                bytes: Cow::Borrowed(&rgba),
            })
            .unwrap();
        let wait = |pending: Pending| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if let Some(result) = pending.poll() {
                    break result;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "clipboard request timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        let imported = wait(Pending::clipboard([0.0; 2], 512.0)).unwrap().unwrap();
        assert_eq!(imported.pixels.2, rgba);
        let mut doc = Document::blank().unwrap();
        let index = doc
            .import_image([0.0; 2], [2.0; 2], &imported.filename, imported.bytes)
            .unwrap();
        let name = doc.nodes[index].attachment.clone().unwrap();
        doc.undo();
        assert!(doc.nodes.is_empty());
        doc.redo();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clipboard.mio");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        let image = image::load_from_memory(&reopened.attachments[&name])
            .unwrap()
            .into_rgba8();
        assert_eq!(image.into_raw(), rgba);
        owner.set_text("plain text").unwrap();
        let result = wait(Pending::clipboard([0.0; 2], 512.0));
        assert!(result.err().unwrap().contains("剪贴板中没有可读取的图片"));
    }
}
