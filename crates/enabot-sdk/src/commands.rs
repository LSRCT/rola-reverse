use crate::auth::now_ms;
use crate::client::MiniSession;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoQuality {
    Fluent,
    Hd,
    Super,
    Original,
}

impl VideoQuality {
    pub fn app_value(self) -> u8 {
        match self {
            Self::Fluent => 1,
            Self::Hd => 2,
            Self::Super => 3,
            Self::Original => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Fluent => "fluent",
            Self::Hd => "hd",
            Self::Super => "super",
            Self::Original => "original",
        }
    }
}

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

pub fn video_quality(session: &MiniSession, quality: VideoQuality) -> Value {
    json!({
        "id": 102055,
        "sid": session.sid,
        "type": 0,
        "timestamp": now_ms(),
        "data": {
            "videoQuality": quality.app_value(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_payload_is_trigger_only() {
        let payload = snapshot();

        assert_eq!(payload["id"], 102011);
        assert_eq!(payload["type"], 0);
        assert!(payload["timestamp"].as_u64().is_some());
        assert!(payload.get("sid").is_none());
        assert!(payload.get("data").is_none());
        assert!(payload.get("url").is_none());
        assert!(payload.get("file").is_none());
    }

    #[test]
    fn video_quality_payload_matches_rola_mini_live_command() {
        let session = MiniSession {
            sid: "session-id".to_string(),
            is_online: true,
            app_rtm_uid: "app-rtm".to_string(),
            app_rtm_token: "app-rtm-token".to_string(),
            app_rtc_uid: "100000001".to_string(),
            app_rtc_token: "app-rtc-token".to_string(),
            mini_rtm_uid: "mini-rtm".to_string(),
            mini_rtc_uid: "200000001".to_string(),
            rtc_channel: "mini-channel".to_string(),
        };
        let payload = video_quality(&session, VideoQuality::Original);

        assert_eq!(payload["id"], 102055);
        assert_eq!(payload["sid"], "session-id");
        assert_eq!(payload["type"], 0);
        assert!(payload["timestamp"].as_u64().is_some());
        assert_eq!(payload["data"]["videoQuality"], 4);
    }
}
