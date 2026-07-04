use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub struct NativeRtmSidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    events: Vec<Value>,
}

impl NativeRtmSidecar {
    pub async fn start(path: &Path) -> Result<Self> {
        let mut child = Command::new("node")
            .arg(path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .with_context(|| format!("starting native RTM sidecar {}", path.display()))?;

        let stdin = child.stdin.take().context("sidecar stdin unavailable")?;
        let stdout = child.stdout.take().context("sidecar stdout unavailable")?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            next_id: 1,
            events: Vec::new(),
        })
    }

    pub async fn connect(&mut self, app_id: &str, uid: &str, token: &str) -> Result<()> {
        self.request(json!({
            "method": "connect",
            "params": {
                "appId": app_id,
                "uid": uid,
                "token": token,
            }
        }))
        .await
        .map(|_| ())
    }

    pub async fn send_user_message(&mut self, peer_id: &str, message: &str) -> Result<()> {
        self.request(json!({
            "method": "send_user_message",
            "params": {
                "peerId": peer_id,
                "message": message,
            }
        }))
        .await
        .map(|_| ())
    }

    pub async fn logout(&mut self) -> Result<()> {
        self.request(json!({ "method": "logout", "params": {} }))
            .await
            .map(|_| ())
    }

    pub async fn collect_for(&mut self, duration: Duration) -> Result<()> {
        let end = tokio::time::Instant::now() + duration;
        loop {
            let now = tokio::time::Instant::now();
            if now >= end {
                break;
            }
            let remaining = end - now;
            match tokio::time::timeout(
                remaining.min(Duration::from_millis(250)),
                self.stdout.next_line(),
            )
            .await
            {
                Ok(Ok(Some(line))) => self.handle_line_without_response(&line)?,
                Ok(Ok(None)) => bail!("native RTM sidecar exited"),
                Ok(Err(err)) => return Err(err).context("reading sidecar event"),
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }

    async fn request(&mut self, mut payload: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        payload["id"] = json!(id);

        let line = serde_json::to_string(&payload)?;
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let deadline = Duration::from_secs(20);
        loop {
            let line = tokio::time::timeout(deadline, self.stdout.next_line())
                .await
                .context("timed out waiting for native RTM sidecar response")?
                .context("reading native RTM sidecar response")?
                .context("native RTM sidecar exited before responding")?;

            let value: Value = serde_json::from_str(&line)
                .with_context(|| format!("sidecar emitted non-JSON line: {line}"))?;
            match value.get("type").and_then(Value::as_str) {
                Some("event") => self.events.push(value),
                Some("response") if value.get("id").and_then(Value::as_u64) == Some(id) => {
                    if value.get("ok").and_then(Value::as_bool) == Some(true) {
                        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                    }
                    let error = value
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("sidecar request failed");
                    return Err(anyhow!("{error}"));
                }
                Some("response") => self.events.push(value),
                _ => return Err(anyhow!("unexpected sidecar line: {value}")),
            }
        }
    }

    fn handle_line_without_response(&mut self, line: &str) -> Result<()> {
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("sidecar emitted non-JSON line: {line}"))?;
        self.events.push(value);
        Ok(())
    }
}

impl Drop for NativeRtmSidecar {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
