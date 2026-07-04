use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use enabot_sdk::commands;
use enabot_sdk::sidecar::NativeRtmSidecar;
use enabot_sdk::{Config, EnabotClient, MiniSession};
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
    Session,
    Wiggle,
    Stop,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
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
            run_wiggle(&client, &args.sidecar, &session).await?;
        }
        Command::Stop => {
            let session = fresh_session(&client).await?;
            ensure_online(&session)?;
            run_stop(&client, &args.sidecar, &session).await?;
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

async fn connect_sidecar(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
) -> Result<NativeRtmSidecar> {
    let mut sidecar = NativeRtmSidecar::start(sidecar_path).await?;
    sidecar
        .connect(
            &client.config().agora_app_id,
            &session.app_rtm_uid,
            &session.app_rtm_token,
        )
        .await
        .context("native RTM login failed")?;
    Ok(sidecar)
}

async fn run_wiggle(
    client: &EnabotClient,
    sidecar_path: &PathBuf,
    session: &MiniSession,
) -> Result<()> {
    let mut sidecar = connect_sidecar(client, sidecar_path, session).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "rtm_login_ok"}))?
    );

    send_command(
        &mut sidecar,
        session,
        "enter_live",
        commands::enter_live(session)?,
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(800)).await;

    send_command(
        &mut sidecar,
        session,
        "nudge_forward",
        commands::drive(session, 55, 0),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(450)).await;

    send_command(&mut sidecar, session, "stop", commands::stop(session)).await?;
    tokio::time::sleep(Duration::from_millis(350)).await;

    send_command(
        &mut sidecar,
        session,
        "nudge_back",
        commands::drive(session, -55, 0),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(450)).await;

    send_command(&mut sidecar, session, "stop", commands::stop(session)).await?;
    sidecar.collect_for(Duration::from_millis(2500)).await?;
    let events = sidecar.take_events();
    let _ = sidecar.logout().await;

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
    let mut sidecar = connect_sidecar(client, sidecar_path, session).await?;
    send_command(&mut sidecar, session, "stop", commands::stop(session)).await?;
    sidecar.collect_for(Duration::from_millis(1000)).await?;
    let events = sidecar.take_events();
    let _ = sidecar.logout().await;
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

async fn send_command(
    sidecar: &mut NativeRtmSidecar,
    session: &MiniSession,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let message = serde_json::to_string(&payload)?;
    sidecar
        .send_user_message(&session.mini_rtm_uid, &message)
        .await
        .with_context(|| format!("sending {action}"))?;
    println!(
        "{}",
        serde_json::to_string(&json!({"step": "send_ok", "action": action}))?
    );
    Ok(())
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
