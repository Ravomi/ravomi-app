//! Private protocol to Ravomi's compiled Pi Agent Core engine.
use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command as Process, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub(super) struct Output {
    pub text: String,
    pub usage: Option<serde_json::Value>,
    pub session_id: Option<String>,
}
pub(super) trait Executor: Send + Sync {
    fn configure(&self, _provider: &str, _key: String) -> Result<()> {
        Err("此执行器不支持凭据配置".into())
    }
    fn execute(
        &self,
        model: &str,
        prompt: &str,
        cwd: &Path,
        cancel: &AtomicBool,
        timeout: u64,
        progress: &dyn Fn(&str),
    ) -> Result<Output>;
}
pub(super) struct Pi {
    executable: std::path::PathBuf,
    credentials: std::sync::Mutex<BTreeMap<String, String>>,
}

impl Pi {
    pub fn bundled() -> Result<Self> {
        let name = if cfg!(windows) {
            "ravomi-agent.exe"
        } else {
            "ravomi-agent"
        };
        let installed = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("应用路径不可用")?
            .join(name);
        if installed.is_file() {
            return Ok(Self {
                executable: installed,
                credentials: Default::default(),
            });
        }
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            _ => return Err("此平台尚无内置 Agent 引擎构建".into()),
        };
        let development = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("app/src-tauri/binaries")
            .join(format!(
                "ravomi-agent-{target}{}",
                if cfg!(windows) { ".exe" } else { "" }
            ));
        if !development.is_file() {
            return Err("缺少内置 Agent 引擎，请先在 app 运行 npm run build:agent".into());
        }
        Ok(Self {
            executable: development,
            credentials: Default::default(),
        })
    }
}

fn configure_process(process: &mut Process) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
}

struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Process group is created by us, and the child is not reaped yet.
            let _ = Process::new("kill")
                .args(["-KILL", "--", &format!("-{}", self.0.id())])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = Process::new("taskkill")
                .args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn capture(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let room = MAX_BYTES.saturating_sub(kept.len());
                    kept.extend_from_slice(&buffer[..n.min(room)]);
                }
                Err(e) => {
                    kept.extend_from_slice(format!("\n读取输出失败：{e}").as_bytes());
                    break;
                }
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    })
}

impl Executor for Pi {
    fn configure(&self, provider: &str, key: String) -> Result<()> {
        if !matches!(provider, "deepseek" | "openai" | "anthropic")
            || key.trim().is_empty()
            || key.len() > 4096
        {
            return Err("提供方或 API Key 格式无效".into());
        }
        self.credentials
            .lock()
            .map_err(|e| e.to_string())?
            .insert(provider.into(), key);
        Ok(())
    }
    fn execute(
        &self,
        model: &str,
        prompt: &str,
        cwd: &Path,
        cancel: &AtomicBool,
        timeout: u64,
        progress: &dyn Fn(&str),
    ) -> Result<Output> {
        let mut process = Process::new(&self.executable);
        if let Some(provider) = model.split('/').next()
            && let Some(key) = self
                .credentials
                .lock()
                .map_err(|e| e.to_string())?
                .get(provider)
        {
            process.env("RAVOMI_AI_API_KEY", key);
        }
        process
            .args(["--model", model])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process(&mut process);
        let mut child = Guard(
            process
                .spawn()
                .map_err(|e| format!("无法启动内置 Pi Agent 引擎：{e}"))?,
        );
        let mut stdin = child.0.stdin.take().ok_or("Pi 输入管道不可用")?;
        let stdout = child.0.stdout.take().ok_or("Pi 输出管道不可用")?;
        let stderr = capture(child.0.stderr.take().ok_or("Pi 错误管道不可用")?);
        let (send, receive) = mpsc::sync_channel::<Result<serde_json::Value>>(64);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = vec![];
                match reader
                    .by_ref()
                    .take((4 * MAX_BYTES + 1) as u64)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => break,
                    Ok(_) if bytes.len() > 4 * MAX_BYTES => {
                        let _ = send.send(Err("Pi 单条事件超过 2 MiB".into()));
                        break;
                    }
                    Ok(_) => {
                        let value = serde_json::from_slice(&bytes)
                            .map_err(|e| format!("Pi RPC 事件解析失败：{e}"));
                        if send.send(value).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = send.send(Err(e.to_string()));
                        break;
                    }
                }
            }
        });
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"id":"state","type":"get_state"})
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"id":"prompt","type":"prompt","message":prompt})
        )
        .map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        let started = Instant::now();
        let mut output = Output {
            text: String::new(),
            usage: None,
            session_id: None,
        };
        let mut error = None;
        loop {
            if cancel.load(Ordering::Acquire) {
                let _ = writeln!(stdin, "{{\"type\":\"abort\"}}");
                return Err("运行已取消".into());
            }
            if started.elapsed().as_secs() >= timeout {
                return Err(format!("Pi 运行超过 {timeout} 秒，已终止本次进程"));
            }
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok(record) => {
                    let record = record?;
                    match record["type"].as_str() {
                        Some("response") if record["success"] == false => {
                            return Err(format!(
                                "Pi 拒绝请求：{}",
                                record["error"].as_str().unwrap_or("未知错误")
                            ));
                        }
                        Some("response") if record["id"] == "state" => {
                            output.session_id =
                                record["data"]["sessionId"].as_str().map(str::to_owned)
                        }
                        Some("message_update")
                            if record["assistantMessageEvent"]["type"] == "text_delta" =>
                        {
                            if let Some(delta) = record["assistantMessageEvent"]["delta"].as_str() {
                                progress(delta);
                            }
                        }
                        Some("message_end") if record["message"]["role"] == "assistant" => {
                            let message = &record["message"];
                            if matches!(message["stopReason"].as_str(), Some("error" | "aborted")) {
                                error = Some(
                                    message["errorMessage"]
                                        .as_str()
                                        .unwrap_or("Pi 模型运行失败")
                                        .to_owned(),
                                );
                            } else {
                                error = None;
                            }
                            output.text = message["content"]
                                .as_array()
                                .map(|blocks| {
                                    blocks
                                        .iter()
                                        .filter(|b| b["type"] == "text")
                                        .filter_map(|b| b["text"].as_str())
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                })
                                .unwrap_or_default();
                            if output.text.len() > MAX_BYTES {
                                return Err("Pi 成果超过 512 KiB".into());
                            }
                            output.usage = message.get("usage").cloned();
                        }
                        Some("agent_settled") => {
                            if let Some(error) = error {
                                return Err(error);
                            }
                            if output.text.trim().is_empty() {
                                return Err("Pi 未返回最终文本".into());
                            }
                            return Ok(output);
                        }
                        Some("tool_execution_start") => {
                            return Err("Pi 在禁用工具模式下尝试执行工具，运行已终止".into());
                        }
                        _ => {}
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    drop(child);
                    return Err(format!(
                        "Pi 在完成事件前退出：{}",
                        stderr.join().unwrap_or_else(|_| "错误输出读取中断".into())
                    ));
                }
            }
        }
    }
}

pub(super) fn test_command(
    argv: &[String],
    cwd: &Path,
    cancel: &AtomicBool,
    timeout: u64,
) -> Result<Output> {
    let executable = argv.first().ok_or("未配置测试命令")?;
    let mut command = Process::new(executable);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Tests are explicitly user-authorized local code, not a security sandbox.
    // Do not propagate provider credentials into the generated program.
    let preserved: Vec<_> = std::env::vars_os()
        .filter(|(k, _)| {
            matches!(
                k.to_str(),
                Some("PATH" | "SystemRoot" | "WINDIR" | "TMP" | "TEMP" | "LANG" | "LC_ALL")
            )
        })
        .collect();
    command.env_clear().envs(preserved).env("CI", "1");
    configure_process(&mut command);
    let mut child = Guard(
        command
            .spawn()
            .map_err(|e| format!("测试命令无法启动：{e}"))?,
    );
    let stdout = capture(child.0.stdout.take().ok_or("测试输出不可用")?);
    let stderr = capture(child.0.stderr.take().ok_or("测试错误输出不可用")?);
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("运行已取消".into());
        }
        if started.elapsed().as_secs() >= timeout {
            return Err("测试超时，已终止进程组".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            drop(child);
            let text = format!(
                "命令：{}\n退出码：{:?}\n{}\n{}",
                argv.join(" "),
                status.code(),
                stdout.join().map_err(|_| "测试输出线程中断")?,
                stderr.join().map_err(|_| "测试错误线程中断")?
            );
            return if status.success() {
                Ok(Output {
                    text,
                    usage: None,
                    session_id: None,
                })
            } else {
                Err(text)
            };
        }
        thread::sleep(Duration::from_millis(100));
    }
}
