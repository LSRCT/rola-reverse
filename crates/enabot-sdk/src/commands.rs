use crate::auth::now_ms;
use crate::client::MiniSession;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub fn user_id_from_session(session: &MiniSession) -> Result<u64> {
    let app_rtc_uid = session
        .app_rtc_uid
        .parse::<u64>()
        .context("app_rtc_uid must be numeric")?;
    if app_rtc_uid < 100_000_000 {
        bail!("app_rtc_uid is too small to derive user id");
    }
    Ok(app_rtc_uid - 100_000_000)
}

pub fn enter_live(session: &MiniSession) -> Result<Value> {
    Ok(json!({
        "id": 101003,
        "sid": session.sid,
        "data": { "userId": user_id_from_session(session)? },
        "type": 0,
        "timestamp": now_ms(),
    }))
}

pub fn heartbeat(session: &MiniSession) -> Value {
    json!({
        "id": 101005,
        "sid": session.sid,
        "data": { "state": 0 },
        "type": 0,
        "timestamp": now_ms(),
    })
}

pub fn drive(session: &MiniSession, ly: i64, rx: i64) -> Value {
    json!({
        "id": 101007,
        "sid": session.sid,
        "data": {
            "lx": 0,
            "ly": ly.clamp(-100, 100),
            "rx": rx.clamp(-100, 100),
            "ry": 0,
            "buttons": 1,
        },
        "type": 0,
        "timestamp": now_ms(),
    })
}

pub fn stop(session: &MiniSession) -> Value {
    drive(session, 0, 0)
}

pub fn snapshot() -> Value {
    json!({
        "id": 102011,
        "type": 0,
        "timestamp": now_ms(),
    })
}
