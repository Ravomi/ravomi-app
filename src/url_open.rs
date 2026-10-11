use super::*;
use std::sync::{Mutex, mpsc};

/// Only explicit web addresses are sent to the desktop opener.
pub fn target(node: &GraphNode) -> Result<String> {
    let source = match node.kind.as_str() {
        "UrlNode" => node.url.as_deref().ok_or("网址节点缺少链接")?,
        "TextNode" => &node.text,
        _ => return Err("请选择网址节点或包含网址的文字节点".into()),
    };
    let source = source.trim();
    let source = source
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(source);
    let Some((scheme, rest)) = source.split_once("://") else {
        return Err("网址须以 https:// 或 http:// 开头".into());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        || authority.is_empty()
        || authority.starts_with(':')
        || source.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("网址无效，请使用完整的 http:// 或 https:// 链接".into());
    }
    Ok(source.to_owned())
}

pub struct Pending(Mutex<mpsc::Receiver<std::result::Result<(), String>>>);

impl Pending {
    pub fn start(doc: &Document, selected: &HashSet<usize>) -> Result<Self> {
        let mut indices: Vec<_> = selected.iter().copied().collect();
        indices.sort_unstable();
        let mut urls = Vec::new();
        for index in indices {
            let url = target(doc.nodes.get(index).ok_or("所选节点不存在")?)?;
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        if urls.is_empty() {
            return Err("请先选择需要打开的网址节点".into());
        }
        Ok(Self::spawn(urls, open))
    }

    fn spawn(
        urls: Vec<String>,
        opener: impl Fn(&str) -> std::result::Result<(), String> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let errors: Vec<_> = urls.iter().filter_map(|url| opener(url).err()).collect();
            let result = if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("\n"))
            };
            let _ = tx.send(result);
        });
        Self(Mutex::new(rx))
    }

    pub fn poll(&self) -> Option<std::result::Result<(), String>> {
        let Ok(receiver) = self.0.lock() else {
            return Some(Err("打开网址的任务状态损坏，请重试".into()));
        };
        match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("打开网址的任务中断，请重试".into())),
        }
    }
}

fn open(url: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    let output = command
        .arg(url)
        .output()
        .map_err(|error| format!("无法启动浏览器：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(format!("浏览器未能打开网址：{}\n{}", url, detail.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_nodes_open_the_destination_instead_of_the_title() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let node = &mut doc.nodes[0];
        node.kind = "UrlNode".into();
        node.text = "网站标题".into();
        node.url = Some("https://example.com/path?q=hello&other=1".into());
        assert_eq!(target(node).unwrap(), node.url.as_deref().unwrap());
        node.kind = "TextNode".into();
        node.text = "  \"https://example.com/中文\"  ".into();
        assert_eq!(target(node).unwrap(), "https://example.com/中文");
        for invalid in [
            "file:///tmp/test",
            "javascript:alert(1)",
            "https://",
            "https:///path",
            "https://example.com\n--help",
            "ordinary text",
        ] {
            node.text = invalid.into();
            assert!(target(node).is_err(), "{invalid}");
        }
    }

    #[test]
    fn asynchronous_opener_errors_are_returned_to_the_editor() {
        let pending = Pending::spawn(vec!["https://example.com".into()], |_| {
            Err("没有默认浏览器".into())
        });
        let result = pending
            .0
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(result, Err("没有默认浏览器".into()));
        let (sender, receiver) = mpsc::channel();
        let pending = Pending(Mutex::new(receiver));
        drop(sender);
        assert!(
            pending.poll().unwrap().is_err(),
            "disconnection is visible too"
        );
    }
}
