use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use enabot_sdk::{
    Config, DEFAULT_LIVE_READY_TIMEOUT_MS, DEFAULT_LIVE_SETTLE_MS, EnabotClient, LiveReadyStatus,
    MiniSession, RolaMiniControl, VideoQuality,
};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "enabot")]
#[command(about = "Phone-free Enabot ROLA Mini control")]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "sidecars/native-rtm/index.js")]
    sidecar: PathBuf,

    #[arg(long, default_value = "sidecars/rtc-snapshot-native-macos")]
    rtc_sidecar: PathBuf,

    #[arg(long, default_value_t = DEFAULT_LIVE_READY_TIMEOUT_MS, global = true)]
    live_ready_timeout_ms: u64,

    #[arg(long, default_value_t = DEFAULT_LIVE_SETTLE_MS, global = true)]
    live_settle_ms: u64,
}

#[derive(Debug, Subcommand)]
enum Command {
    Login,
    Robots,
    Session,
    Wiggle,
    Drive(DriveArgs),
    Forward(TimedSpeedArgs),
    Backward(TimedSpeedArgs),
    TurnLeft(TimedSpeedArgs),
    TurnRight(TimedSpeedArgs),
    Stop,
    Snapshot(SnapshotArgs),
}

#[derive(Debug, Parser)]
struct DriveArgs {
    #[arg(long)]
    ly: i64,

    #[arg(long)]
    rx: i64,

    #[arg(long, default_value_t = 500)]
    ms: u64,
}

#[derive(Debug, Parser)]
struct TimedSpeedArgs {
    #[arg(long, default_value_t = 55)]
    speed: i64,

    #[arg(long, default_value_t = 500)]
    ms: u64,
}

#[derive(Debug, Parser)]
struct SnapshotArgs {
    #[arg(long, default_value = "artifacts/snapshots/latest.jpg")]
    out: PathBuf,

    #[arg(long)]
    quality: Option<SnapshotQuality>,

    #[arg(long, default_value_t = 30_000)]
    wait_ms: u64,

    #[arg(long, default_value = "rtc")]
    rtc_mode: String,

    #[arg(long, default_value = "h264")]
    codec: String,
}

#[derive(Clone, Copy, Debug)]
struct LiveReadyConfig {
    timeout: Duration,
    min_settle: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let sidecar = args.sidecar.clone();
    let rtc_sidecar = args.rtc_sidecar.clone();
    let live_ready = checked_live_ready_config(args.live_ready_timeout_ms, args.live_settle_ms)?;
    let config = Config::load()?;
    let client = EnabotClient::new(config)?;

    match args.command {
        Command::Login => {
            let login = client.login().await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "cookieNames": login.cookie_names,
                }))?
            );
        }
        Command::Robots => {
            let login = client.login().await?;
            let robots = client.robots(&login).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "robots": robots,
                }))?
            );
        }
        Command::Session => {
            let session = fresh_session(&client).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&session_summary(&session))?
            );
        }
        Command::Wiggle => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_wiggle(&client, &sidecar, &session, live_ready).await?;
        }
        Command::Drive(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client, &sidecar, &session, live_ready, drive.ly, drive.rx, drive.ms, "drive",
            )
            .await?;
        }
        Command::Forward(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                speed(drive.speed),
                0,
                drive.ms,
                "forward",
            )
            .await?;
        }
        Command::Backward(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                -speed(drive.speed),
                0,
                drive.ms,
                "backward",
            )
            .await?;
        }
        Command::TurnLeft(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                0,
                -speed(drive.speed),
                drive.ms,
                "turn_left",
            )
            .await?;
        }
        Command::TurnRight(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client,
                &sidecar,
                &session,
                live_ready,
                0,
                speed(drive.speed),
                drive.ms,
                "turn_right",
            )
            .await?;
        }
        Command::Stop => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_stop(&client, &sidecar, &session).await?;
        }
        Command::Snapshot(snapshot) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_snapshot(
                &client,
                &sidecar,
                &rtc_sidecar,
                &session,
                &snapshot,
                live_ready,
            )
            .await?;
        }
    }

    Ok(())
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
) -> Result<()> {
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "rtm_login_ok"}))?
    );

    enter_live_ready(&mut robot, live_ready).await?;

    robot.drive_for(55, 0, Duration::from_millis(450)).await?;
    print_send_ok("nudge_forward")?;
    print_send_ok("stop")?;
    tokio::time::sleep(Duration::from_millis(350)).await;

    robot.drive_for(-55, 0, Duration::from_millis(450)).await?;
    print_send_ok("nudge_back")?;
    print_send_ok("stop")?;
    tokio::time::sleep(Duration::from_millis(350)).await;

    robot.collect_for(Duration::from_millis(2500)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "step": "wiggle_done",
            "robotPeer": session.mini_rtm_uid,
            "eventCount": events.len(),
            "events": summarize_events(&events),
        }))?
    );
    Ok(())
}

async fn run_stop(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
) -> Result<()> {
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    robot.stop().await?;
    print_send_ok("stop")?;
    robot.collect_for(Duration::from_millis(1000)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "step": "stop_done",
            "eventCount": events.len(),
            "events": summarize_events(&events),
        }))?
    );
    Ok(())
}

async fn run_snapshot(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    rtc_sidecar_path: &PathBuf,
    session: &MiniSession,
    args: &SnapshotArgs,
    live_ready: LiveReadyConfig,
) -> Result<()> {
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "rtm_login_ok"}))?
    );

    enter_live_ready(&mut robot, live_ready).await?;

    apply_snapshot_quality(&mut robot, args).await?;

    robot.snapshot_trigger().await?;
    print_send_ok("snapshot_trigger")?;
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

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "step": "snapshot_done",
            "out": args.out,
            "capture": capture,
            "eventCount": events.len(),
            "events": summarize_events(&events),
        }))?
    );
    Ok(())
}

async fn enter_live_ready(robot: &mut RolaMiniControl, live_ready: LiveReadyConfig) -> Result<()> {
    robot.enter_live().await?;
    print_send_ok("enter_live")?;
    let status = robot
        .wait_for_live_control_ready(live_ready.timeout, live_ready.min_settle)
        .await?;
    print_live_ready(&status)
}

async fn apply_snapshot_quality(robot: &mut RolaMiniControl, args: &SnapshotArgs) -> Result<()> {
    let Some(quality) = args.quality.map(VideoQuality::from) else {
        return Ok(());
    };

    robot.set_video_quality(quality).await?;
    let action = format!("video_quality_{}", quality.name());
    print_send_ok(&action)?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    Ok(())
}

async fn run_rtc_snapshot_capture_with_retries(
    robot: &mut RolaMiniControl,
    rtc_sidecar_path: &PathBuf,
    client: &EnabotClient,
    session: &MiniSession,
    args: &SnapshotArgs,
    live_ready: LiveReadyConfig,
) -> Result<serde_json::Value> {
    let mut last_error = None;

    for attempt in 0..3 {
        if attempt > 0 {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "step": "snapshot_capture_retry",
                    "attempt": attempt + 1,
                }))?
            );
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

async fn run_drive(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
    live_ready: LiveReadyConfig,
    ly: i64,
    rx: i64,
    ms: u64,
    action: &str,
) -> Result<()> {
    let duration = checked_duration(ms)?;
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "rtm_login_ok"}))?
    );
    enter_live_ready(&mut robot, live_ready).await?;

    robot.drive_for(ly, rx, duration).await?;
    print_send_ok(action)?;
    print_send_ok("stop")?;

    robot.collect_for(Duration::from_millis(1200)).await?;
    let events = robot.take_events();
    let _ = robot.logout().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "step": "drive_done",
            "action": action,
            "ly": ly.clamp(-100, 100),
            "rx": rx.clamp(-100, 100),
            "ms": duration.as_millis(),
            "eventCount": events.len(),
            "events": summarize_events(&events),
        }))?
    );
    Ok(())
}

fn run_rtc_snapshot_sidecar(
    sidecar_path: &PathBuf,
    client: &EnabotClient,
    session: &MiniSession,
    args: &SnapshotArgs,
) -> Result<serde_json::Value> {
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
            .ok_or_else(|| anyhow::anyhow!("RTC snapshot sidecar stdin unavailable"))?;
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

fn print_send_ok(action: &str) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "send_ok", "action": action}))?
    );
    Ok(())
}

fn print_live_ready(status: &LiveReadyStatus) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string(&json!({
            "step": "live_ready",
            "confirmed": status.confirmed,
            "eventId": status.event_id,
            "elapsedMs": status.elapsed.as_millis(),
        }))?
    );
    Ok(())
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

fn session_summary(session: &MiniSession) -> serde_json::Value {
    json!({
        "sid": session.sid,
        "isOnline": session.is_online,
        "appRtmUid": session.app_rtm_uid,
        "miniRtmUid": session.mini_rtm_uid,
        "rtcChannel": session.rtc_channel,
    })
}

fn summarize_events(events: &[serde_json::Value]) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter_map(|event| {
            let name = event.get("event").and_then(|value| value.as_str())?;
            Some(json!({
                "event": name,
                "publisher": event.get("publisher"),
                "channelName": event.get("channelName"),
                "messagePreview": event.get("message").and_then(|message| message.as_str()).unwrap_or("").chars().take(160).collect::<String>(),
            }))
        })
        .take(12)
        .collect()
}
