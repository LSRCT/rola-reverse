use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use enabot_sdk::{Config, EnabotClient, MiniSession, RolaMiniControl};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "enabot")]
#[command(about = "Phone-free Enabot ROLA Mini control")]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "sidecars/native-rtm/index.js")]
    sidecar: PathBuf,
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let sidecar = args.sidecar.clone();
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
            run_wiggle(&client, &sidecar, &session).await?;
        }
        Command::Drive(drive) => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_drive(
                &client, &sidecar, &session, drive.ly, drive.rx, drive.ms, "drive",
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
) -> Result<()> {
    let mut robot = RolaMiniControl::connect(client, sidecar_path, session.clone()).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "rtm_login_ok"}))?
    );

    robot.enter_live().await?;
    print_send_ok("enter_live")?;
    tokio::time::sleep(Duration::from_millis(800)).await;

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

async fn run_drive(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
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
    robot.enter_live().await?;
    print_send_ok("enter_live")?;
    tokio::time::sleep(Duration::from_millis(250)).await;

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

fn print_send_ok(action: &str) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "send_ok", "action": action}))?
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
