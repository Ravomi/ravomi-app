use super::{Document, Result, theme};
use bevy_egui::egui;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

type Outcome = std::result::Result<Option<PathBuf>, String>;

pub struct Export {
    pub open: bool,
    pub selection: bool,
    width: u32,
    transparent: bool,
    pending: Option<Mutex<mpsc::Receiver<Outcome>>>,
    cancel: Arc<AtomicBool>,
    status: String,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            open: false,
            selection: false,
            width: 2048,
            transparent: false,
            pending: None,
            cancel: Arc::new(AtomicBool::new(false)),
            status: String::new(),
        }
    }
}

impl Drop for Export {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Export {
    pub fn status_button(&mut self, ui: &mut egui::Ui) {
        if self.pending.is_some() || !self.status.is_empty() {
            let label = if self.pending.is_some() {
                "图片导出中…"
            } else {
                "查看导出结果"
            };
            if ui.button(label).clicked() {
                self.open = true;
            }
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        light: bool,
        selected: bool,
    ) -> (bool, Option<String>) {
        let mut error = None;
        if let Some(receiver) = &self.pending {
            ctx.request_repaint_after(Duration::from_millis(100));
            let outcome = match receiver.lock() {
                Ok(receiver) => match receiver.try_recv() {
                    Ok(value) => Some(value),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("导出任务意外结束，请重试".into()))
                    }
                },
                Err(_) => Some(Err("无法读取导出任务状态，请重试".into())),
            };
            if let Some(outcome) = outcome {
                self.pending = None;
                match outcome {
                    Ok(Some(path)) => self.status = format!("已导出：{}", path.display()),
                    Ok(None) => self.status = "已取消导出".into(),
                    Err(message) => {
                        self.status = "导出未完成，可重试。".into();
                        error = Some(message);
                    }
                }
            }
        }
        if !self.open {
            return (false, error);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.open = false;
            return (false, error);
        }
        let mut start = false;
        egui::Modal::new("png-export".into())
            .frame(theme::tool_frame(light))
            .show(ctx, |ui| {
                theme::tool_style(ui, light);
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                ui.set_width((ctx.content_rect().width() - 64.0).clamp(200.0, 420.0));
                self.open = !theme::tool_header(ui, "导出 PNG 图片", light);
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 150.0).max(100.0))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(self.pending.is_none(), |ui| {
                            egui::Grid::new("png-export-options").show(ui, |ui| {
                                ui.label("导出范围");
                                ui.vertical(|ui| {
                                    ui.radio_value(&mut self.selection, false, "整个画布");
                                    ui.add_enabled_ui(selected, |ui| {
                                        ui.radio_value(&mut self.selection, true, "所选内容");
                                    });
                                });
                                ui.end_row();
                                ui.label("图片宽度");
                                ui.add(egui::DragValue::new(&mut self.width).range(64..=8192).suffix(" px"));
                                ui.end_row();
                                ui.label("背景");
                                ui.checkbox(&mut self.transparent, "透明背景");
                                ui.end_row();
                            });
                            ui.weak("保持画布比例，自动包含曲线和文字边界。颜色沿用当前主题。最大 8192 像素单边、1677 万像素。");
                            if self.selection {
                                ui.weak("包含分组成员、所选连线的端点，以及选中节点之间的连线。");
                            }
                            start = ui.add_enabled(!self.selection || selected, theme::primary_button("选择位置并导出…", light)).clicked();
                        });
                        if self.pending.is_some() {
                            ui.label("正在导出，可关闭此窗口继续编辑。");
                            if ui.add_enabled(!self.cancel.load(Ordering::Relaxed), egui::Button::new("取消导出")).clicked() {
                                self.cancel.store(true, Ordering::Relaxed);
                                self.status = "正在取消；若保存对话框仍打开，请先关闭它。".into();
                            }
                        }
                        if !self.status.is_empty() {
                            ui.label(&self.status);
                        }
                    });
            });
        (start, error)
    }

    pub fn start(&mut self, document: Document, font: Vec<u8>, light: bool) {
        if self.pending.is_some() {
            return;
        }
        let width = self.width;
        let transparent = self.transparent;
        self.status.clear();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(Mutex::new(receiver));
        std::thread::spawn(move || {
            let outcome = (|| -> Result<Option<PathBuf>> {
                let filename = document
                    .path
                    .as_ref()
                    .and_then(|path| path.file_stem())
                    .and_then(|name| name.to_str())
                    .unwrap_or("未命名画布");
                let Some(path) = rfd::FileDialog::new()
                    .set_title("导出 PNG 图片")
                    .set_file_name(format!("{filename}.png"))
                    .add_filter("PNG 图片", &["png"])
                    .save_file()
                else {
                    return Ok(None);
                };
                if path
                    .extension()
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("png"))
                {
                    return Err("请使用 .png 扩展名保存图片".into());
                }
                render(&document, &font, &path, width, transparent, light, &cancel)
            })()
            .map_err(|error| error.to_string());
            // Closing the app may discard the result after the worker has stopped.
            let _ = sender.send(outcome);
        });
    }
}

fn existing(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(128 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 128 * 1024 * 1024 {
                return Err("目标文件超过 128 MiB，请选择新文件名".into());
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn render(
    document: &Document,
    font: &[u8],
    path: &Path,
    width: u32,
    transparent: bool,
    light: bool,
    cancel: &AtomicBool,
) -> Result<Option<PathBuf>> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let baseline = existing(path)?;
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("scene.prg");
    let output = directory.path().join("image.png");
    let font_path = directory.path().join("font.ttf");
    document.save_snapshot(&input)?;
    fs::write(&font_path, font)?;
    let log_path = directory.path().join("renderer.log");
    let log = fs::File::create(&log_path)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg(input)
        .arg("--render-png")
        .arg(&output)
        .arg("--export-width")
        .arg(width.to_string())
        .arg("--font")
        .arg(font_path)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if transparent {
        command.arg("--transparent");
    }
    if light {
        command.arg("--light");
    }
    let Some(status) = wait_renderer(&mut command.spawn()?, cancel, Duration::from_secs(150))?
    else {
        return Ok(None);
    };
    if !status.success() {
        let log = fs::read_to_string(log_path)?;
        let tail: String = log
            .chars()
            .rev()
            .take(1500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!("图片渲染失败：{tail}").into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    Ok(publish(&output, path, baseline.as_deref(), cancel)?.then(|| path.to_owned()))
}

fn stop_renderer(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        // The renderer may exit between try_wait and kill, particularly on Windows.
        if let Err(error) = child.kill()
            && child.try_wait()?.is_none()
        {
            return Err(format!("无法停止图片渲染进程：{error}").into());
        }
    }
    child.wait()?;
    Ok(())
}

fn wait_renderer(
    child: &mut Child,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            stop_renderer(child)?;
            return Ok(None);
        }
        if started.elapsed() >= timeout {
            stop_renderer(child)?;
            return Err("导出超时，请降低图片尺寸后重试".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                stop_renderer(child)?;
                return Err(error.into());
            }
        }
    }
}

fn publish(
    output: &Path,
    path: &Path,
    baseline: Option<&[u8]>,
    cancel: &AtomicBool,
) -> Result<bool> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut image = fs::File::open(output)?;
    std::io::copy(&mut image, &mut temp)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(false);
    }
    if existing(path)?.as_deref() != baseline {
        return Err("目标文件在导出期间发生变化，请选择其他文件名重试".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(false);
    }
    if baseline.is_some() {
        temp.persist(path)?;
    } else {
        temp.persist_noclobber(path)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishing_preserves_changed_destination_and_replaces_confirmed_version() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("render.png");
        let target = dir.path().join("chosen.png");
        let cancel = AtomicBool::new(false);
        fs::write(&output, b"rendered").unwrap();
        assert!(publish(&output, &target, None, &cancel).unwrap());
        assert_eq!(fs::read(&target).unwrap(), b"rendered");
        fs::write(&target, b"changed").unwrap();
        assert!(publish(&output, &target, Some(b"rendered"), &cancel).is_err());
        assert!(publish(&output, &target, None, &cancel).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"changed");
        assert!(publish(&output, &target, Some(b"changed"), &cancel).unwrap());
        assert_eq!(fs::read(&target).unwrap(), b"rendered");
    }

    #[test]
    fn cancelled_publication_preserves_destination_and_removes_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("render.png");
        let target = dir.path().join("chosen.png");
        fs::write(&output, b"rendered").unwrap();
        let cancel = AtomicBool::new(true);
        assert!(!publish(&output, &target, None, &cancel).unwrap());
        assert!(!target.exists());
        fs::write(&target, b"original").unwrap();
        assert!(!publish(&output, &target, Some(b"original"), &cancel).unwrap());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_and_timed_out_renderers_are_reaped() {
        for cancelled in [true, false] {
            let mut child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
            assert!(child.try_wait().unwrap().is_none());
            let outcome = wait_renderer(&mut child, &AtomicBool::new(cancelled), Duration::ZERO);
            if cancelled {
                assert!(outcome.unwrap().is_none());
            } else {
                assert!(outcome.unwrap_err().to_string().contains("超时"));
            }
            assert!(!child.try_wait().unwrap().unwrap().success());
        }
    }

    #[cfg(unix)]
    #[test]
    fn renderer_exit_status_survives_wait_and_cancelling_an_exited_child() {
        for (program, success) in [("/bin/true", true), ("/bin/false", false)] {
            let mut child = Command::new(program).spawn().unwrap();
            let status = wait_renderer(&mut child, &AtomicBool::new(false), Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(status.success(), success);
            assert!(
                wait_renderer(&mut child, &AtomicBool::new(true), Duration::ZERO)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(child.try_wait().unwrap(), Some(status));
        }
    }
}
