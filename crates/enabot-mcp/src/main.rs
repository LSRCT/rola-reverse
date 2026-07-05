use anyhow::{Context, Result, bail};
use clap::Parser;
use enabot_sdk::{
    Config, DEFAULT_LIVE_READY_TIMEOUT_MS, DEFAULT_LIVE_SETTLE_MS, EnabotClient, LiveReadyStatus,
    MiniSession, RolaMiniControl, VideoQuality,
};
use rmcp::{
    Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
#[command(name = "enabot-mcp")]
#[command(about = "MCP server for Enabot ROLA Mini control")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8788)]
    port: u16,

    #[arg(long = "allowed-host", default_values_t = default_allowed_hosts())]
    allowed_hosts: Vec<String>,

    #[arg(long, default_value = "sidecars/native-rtm/index.js")]
    sidecar: PathBuf,

    #[arg(long, default_value = "sidecars/rtc-snapshot-native-macos")]
    rtc_sidecar: PathBuf,

    #[arg(long, default_value_t = DEFAULT_LIVE_READY_TIMEOUT_MS)]
    live_ready_timeout_ms: u64,

    #[arg(long, default_value_t = DEFAULT_LIVE_SETTLE_MS)]
    live_settle_ms: u64,
}

#[derive(Debug, Clone)]
struct EnabotMcp {
    sidecar: PathBuf,
    rtc_sidecar: PathBuf,
    live_ready: LiveReadyConfig,
    tool_router: ToolRouter<Self>,
}

#[derive(Clone, Copy, Debug)]
struct LiveReadyConfig {
    timeout: Duration,
    min_settle: Duration,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DriveRequest {
    #[schemars(description = "Forward/backward joystick value from -100 to 100")]
    ly: i64,
    #[schemars(description = "Left/right turn joystick value from -100 to 100")]
    rx: i64,
    #[schemars(description = "Drive duration in milliseconds")]
    ms: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TimedSpeedRequest {
    #[schemars(description = "Movement speed from 0 to 100")]
    speed: Option<i64>,
    #[schemars(description = "Movement duration in milliseconds")]
    ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SnapshotRequest {
    #[schemars(description = "Output JPEG path on the MCP host")]
    out: Option<PathBuf>,
    #[schemars(description = "Snapshot video quality: fluent, hd, or original")]
    quality: Option<SnapshotQuality>,
    #[schemars(description = "Maximum wait for a frame in milliseconds")]
    wait_ms: Option<u64>,
    #[schemars(description = "RTC sidecar mode")]
    rtc_mode: Option<String>,
    #[schemars(description = "Video codec")]
    codec: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum SnapshotQuality {
    Fluent,
    Hd,
    Original,
}

impl From<SnapshotQuality> for VideoQuality {
    fn from(value: SnapshotQuality) -> Self {
        match value {
            SnapshotQuality::Fluent => Self::Fluent,
            SnapshotQuality::Hd => Self::Hd,
            SnapshotQuality::Original => Self::Original,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ToolOutput {
    result: Value,
}

#[derive(Debug)]
struct SnapshotArgs {
    out: PathBuf,
    quality: Option<SnapshotQuality>,
    wait_ms: u64,
    rtc_mode: String,
    codec: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("parsing bind address")?;
    let sidecar = args.sidecar.clone();
    let rtc_sidecar = args.rtc_sidecar.clone();
    let live_ready = checked_live_ready_config(args.live_ready_timeout_ms, args.live_settle_ms)?;

    let service: StreamableHttpService<EnabotMcp, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(EnabotMcp::new(
                sidecar.clone(),
                rtc_sidecar.clone(),
                live_ready,
            ))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_allowed_hosts(args.allowed_hosts),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("enabot MCP server listening at http://{addr}/mcp");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}

fn default_allowed_hosts() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        "rola-mcp.alex-netsch.com".to_string(),
    ]
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EnabotMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("enabot-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions("Control an Enabot ROLA Mini robot.")
    }
}

#[tool_router(router = tool_router)]
impl EnabotMcp {
    fn new(sidecar: PathBuf, rtc_sidecar: PathBuf, live_ready: LiveReadyConfig) -> Self {
        Self {
            sidecar,
            rtc_sidecar,
            live_ready,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List robots available to the configured Enabot account")]
    async fn list_robots(&self) -> Result<Json<ToolOutput>, String> {
        self.run_json(|client, _sidecar, _rtc_sidecar| async move {
            let login = client.login().await?;
            let robots = client.robots(&login).await?;
            Ok(json!({ "robots": robots }))
        })
        .await
    }

    #[tool(description = "Get the current configured robot session and online status")]
    async fn status(&self) -> Result<Json<ToolOutput>, String> {
        self.run_json(|client, _sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            Ok(session_summary(&session))
        })
        .await
    }

    #[tool(description = "Drive with explicit joystick values")]
    async fn drive(
        &self,
        Parameters(req): Parameters<DriveRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_drive(
                &client, &sidecar, &session, live_ready, req.ly, req.rx, req.ms, "drive",
            )
            .await
        })
        .await
    }

    #[tool(description = "Drive forward")]
    async fn forward(
        &self,
        Parameters(req): Parameters<TimedSpeedRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let speed = speed(req.speed.unwrap_or(55));
        let ms = req.ms.unwrap_or(500);
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_drive(
                &client, &sidecar, &session, live_ready, speed, 0, ms, "forward",
            )
            .await
        })
        .await
    }

    #[tool(description = "Drive backward")]
    async fn backward(
        &self,
        Parameters(req): Parameters<TimedSpeedRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let speed = speed(req.speed.unwrap_or(55));
        let ms = req.ms.unwrap_or(500);
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_drive(
                &client, &sidecar, &session, live_ready, -speed, 0, ms, "backward",
            )
            .await
        })
        .await
    }

    #[tool(description = "Turn left")]
    async fn turn_left(
        &self,
        Parameters(req): Parameters<TimedSpeedRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let speed = speed(req.speed.unwrap_or(55));
        let ms = req.ms.unwrap_or(500);
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                0,
                -speed,
                ms,
                "turn_left",
            )
            .await
        })
        .await
    }

    #[tool(description = "Turn right")]
    async fn turn_right(
        &self,
        Parameters(req): Parameters<TimedSpeedRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let speed = speed(req.speed.unwrap_or(55));
        let ms = req.ms.unwrap_or(500);
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                0,
                speed,
                ms,
                "turn_right",
            )
            .await
        })
        .await
    }

    #[tool(description = "Stop the robot")]
    async fn stop(&self) -> Result<Json<ToolOutput>, String> {
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_stop(&client, &sidecar, &session).await
        })
        .await
    }

    #[tool(description = "Run a short forward/backward wiggle")]
    async fn wiggle(&self) -> Result<Json<ToolOutput>, String> {
        let live_ready = self.live_ready;
        self.run_json(|client, sidecar, _rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_wiggle(&client, &sidecar, &session, live_ready).await
        })
        .await
    }

    #[tool(description = "Capture a robot snapshot to a JPEG on the MCP host")]
    async fn snapshot(
        &self,
        Parameters(req): Parameters<SnapshotRequest>,
    ) -> Result<Json<ToolOutput>, String> {
        let args = SnapshotArgs {
            out: req
                .out
                .unwrap_or_else(|| PathBuf::from("artifacts/snapshots/latest.jpg")),
            quality: req.quality,
            wait_ms: req.wait_ms.unwrap_or(30_000),
            rtc_mode: req.rtc_mode.unwrap_or_else(|| "rtc".to_string()),
            codec: req.codec.unwrap_or_else(|| "h264".to_string()),
        };
        let live_ready = self.live_ready;

        self.run_json(|client, sidecar, rtc_sidecar| async move {
            let session = fresh_session(&client).await?;
            run_snapshot(&client, &sidecar, &rtc_sidecar, &session, &args, live_ready).await
        })
        .await
    }

    async fn run_json<F, Fut>(&self, f: F) -> Result<Json<ToolOutput>, String>
    where
        F: FnOnce(EnabotClient, PathBuf, PathBuf) -> Fut,
        Fut: std::future::Future<Output = Result<Value>>,
    {
        let config = Config::load().map_err(|err| format!("{err:#}"))?;
        let client = EnabotClient::new(config).map_err(|err| format!("{err:#}"))?;
        f(client, self.sidecar.clone(), self.rtc_sidecar.clone())
            .await
            .map(|result| Json(ToolOutput { result }))
            .map_err(|err| format!("{err:#}"))
    }
}

async fn fresh_session(client: &EnabotClient) -> Result<MiniSession> {
    let login = client.login().await?;
    client.mini_session(&login).await
}

fn ensure_online(session: &MiniSession) -> Result<()> {
    if !session.is_online {
        bail!("robot is not online according to Mini session endpoint");
    }
    Ok(())
}

async fn run_wiggle(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
    live_ready: LiveReadyConfig,
) -> Result<Value> {
    ensure_online(session)?;
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    let live_ready_status = enter_live_ready(&mut robot, live_ready).await?;

    robot.drive_for(55, 0, Duration::from_millis(450)).await?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    robot.drive_for(-55, 0, Duration::from_millis(450)).await?;
    tokio::time::sleep(Duration::from_millis(350)).await;

    robot.collect_for(Duration::from_millis(2500)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;

    Ok(json!({
        "step": "wiggle_done",
        "robotPeer": session.mini_rtm_uid,
        "liveReady": live_ready_summary(&live_ready_status),
        "eventCount": events.len(),
        "events": summarize_events(&events),
    }))
}

async fn run_stop(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
) -> Result<Value> {
    ensure_online(session)?;
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    robot.stop().await?;
    robot.collect_for(Duration::from_millis(1000)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;
    Ok(json!({
        "step": "stop_done",
        "eventCount": events.len(),
        "events": summarize_events(&events),
    }))
}

async fn enter_live_ready(
    robot: &mut RolaMiniControl,
    live_ready: LiveReadyConfig,
) -> Result<LiveReadyStatus> {
    robot.enter_live().await?;
    robot
        .wait_for_live_control_ready(live_ready.timeout, live_ready.min_settle)
        .await
}

async fn apply_snapshot_quality(
    robot: &mut RolaMiniControl,
    args: &SnapshotArgs,
) -> Result<Option<String>> {
    let Some(quality) = args.quality.map(VideoQuality::from) else {
        return Ok(None);
    };

    robot.set_video_quality(quality).await?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    Ok(Some(quality.name().to_string()))
}

async fn run_drive(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
    live_ready: LiveReadyConfig,
    ly: i64,
    rx: i64,
    ms: u64,
    action: &str,
) -> Result<Value> {
    ensure_online(session)?;
    let duration = checked_duration(ms)?;
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    let live_ready_status = enter_live_ready(&mut robot, live_ready).await?;

    robot.drive_for(ly, rx, duration).await?;
    robot.collect_for(Duration::from_millis(1200)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;

    Ok(json!({
        "step": "drive_done",
        "action": action,
        "ly": ly.clamp(-100, 100),
        "rx": rx.clamp(-100, 100),
        "ms": duration.as_millis(),
        "liveReady": live_ready_summary(&live_ready_status),
        "eventCount": events.len(),
        "events": summarize_events(&events),
    }))
}

async fn run_snapshot(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    rtc_sidecar_path: &PathBuf,
    session: &MiniSession,
    args: &SnapshotArgs,
    live_ready: LiveReadyConfig,
) -> Result<Value> {
    ensure_online(session)?;
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    let live_ready_status = enter_live_ready(&mut robot, live_ready).await?;
    let quality = apply_snapshot_quality(&mut robot, args).await?;

    robot.snapshot_trigger().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let capture = run_rtc_snapshot_capture_with_retries(
        &mut robot,
        rtc_sidecar_path,
        client,
        session,
        args,
        live_ready,
    )
    .await?;

    robot.collect_for(Duration::from_millis(1200)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;

    Ok(json!({
        "step": "snapshot_done",
        "out": args.out,
        "quality": quality,
        "liveReady": live_ready_summary(&live_ready_status),
        "capture": capture,
        "eventCount": events.len(),
        "events": summarize_events(&events),
    }))
}

async fn run_rtc_snapshot_capture_with_retries(
    robot: &mut RolaMiniControl,
    rtc_sidecar_path: &PathBuf,
    client: &EnabotClient,
    session: &MiniSession,
    args: &SnapshotArgs,
    live_ready: LiveReadyConfig,
) -> Result<Value> {
    let mut last_error = None;

    for attempt in 0..3 {
        if attempt > 0 {
            enter_live_ready(robot, live_ready).await?;
            apply_snapshot_quality(robot, args).await?;
            robot.snapshot_trigger().await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        match run_rtc_snapshot_sidecar(rtc_sidecar_path, client, session, args) {
            Ok(capture) => return Ok(capture),
            Err(err) if attempt < 2 => {
                last_error = Some(err);
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_error.expect("retry loop must store an error"))
}

fn run_rtc_snapshot_sidecar(
    sidecar_path: &PathBuf,
    client: &EnabotClient,
    session: &MiniSession,
    args: &SnapshotArgs,
) -> Result<Value> {
    let wait_ms = checked_snapshot_wait(args.wait_ms)?;
    let payload = json!({
        "appId": client.config().agora_app_id,
        "uid": session.app_rtc_uid,
        "token": session.app_rtc_token,
        "channel": session.rtc_channel,
        "expectedPublisher": session.mini_rtc_uid,
        "out": args.out,
        "waitMs": wait_ms,
        "mode": args.rtc_mode,
        "codec": args.codec,
    });

    let mut child = snapshot_sidecar_command(sidecar_path)?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting RTC snapshot sidecar {}", sidecar_path.display()))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("RTC snapshot sidecar stdin unavailable")?;
        stdin.write_all(serde_json::to_string(&payload)?.as_bytes())?;
        stdin.write_all(b"\n")?;
    }

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() {
        let detail = stdout.trim();
        if detail.is_empty() {
            bail!("RTC snapshot sidecar failed with status {}", output.status);
        }
        bail!(
            "RTC snapshot sidecar failed with status {}: {}",
            output.status,
            detail
        );
    }
    serde_json::from_str(stdout.trim()).map_err(Into::into)
}

fn snapshot_sidecar_command(sidecar_path: &PathBuf) -> Result<ProcessCommand> {
    if is_swift_package(sidecar_path)? {
        let mut command = ProcessCommand::new("swift");
        command
            .arg("run")
            .arg("--quiet")
            .arg("--package-path")
            .arg(sidecar_path)
            .arg("RtcSnapshotNativeMac");
        return Ok(command);
    }

    Ok(ProcessCommand::new(sidecar_path))
}

fn is_swift_package(path: &PathBuf) -> Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(path.join("Package.swift").is_file()),
        Ok(_) => Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

fn speed(speed: i64) -> i64 {
    speed.abs().clamp(0, 100)
}

fn checked_duration(ms: u64) -> Result<Duration> {
    if ms > 10_000 {
        bail!("refusing to drive for more than 10000ms in one command");
    }
    Ok(Duration::from_millis(ms))
}

fn checked_live_ready_config(timeout_ms: u64, settle_ms: u64) -> Result<LiveReadyConfig> {
    if timeout_ms > 10_000 {
        bail!("refusing to wait more than 10000ms for live-ready confirmation");
    }
    if settle_ms > 10_000 {
        bail!("refusing to settle live mode for more than 10000ms");
    }
    Ok(LiveReadyConfig {
        timeout: Duration::from_millis(timeout_ms),
        min_settle: Duration::from_millis(settle_ms),
    })
}

fn checked_snapshot_wait(ms: u64) -> Result<u64> {
    if ms > 120_000 {
        bail!("refusing to wait more than 120000ms for one snapshot");
    }
    Ok(ms)
}

fn session_summary(session: &MiniSession) -> Value {
    json!({
        "sid": session.sid,
        "isOnline": session.is_online,
        "appRtmUid": session.app_rtm_uid,
        "miniRtmUid": session.mini_rtm_uid,
        "rtcChannel": session.rtc_channel,
    })
}

fn live_ready_summary(status: &LiveReadyStatus) -> Value {
    json!({
        "confirmed": status.confirmed,
        "eventId": status.event_id,
        "elapsedMs": status.elapsed.as_millis(),
    })
}

fn summarize_events(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter_map(|event| {
            let name = event.get("event").and_then(Value::as_str)?;
            Some(json!({
                "event": name,
                "publisher": event.get("publisher"),
                "channelName": event.get("channelName"),
                "messagePreview": event.get("message").and_then(Value::as_str).unwrap_or("").chars().take(160).collect::<String>(),
            }))
        })
        .take(12)
        .collect()
}
