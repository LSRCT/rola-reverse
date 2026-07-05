use crate::client::{EnabotClient, MiniSession};
use crate::commands::{self, VideoQuality};
use crate::sidecar::NativeRtmSidecar;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_LIVE_READY_TIMEOUT_MS: u64 = 250;
pub const DEFAULT_LIVE_SETTLE_MS: u64 = 800;
const DRIVE_FRAME_INTERVAL_MS: u64 = 100;

#[derive(Clone, Debug)]
pub struct LiveReadyStatus {
    pub confirmed: bool,
    pub event_id: Option<i64>,
    pub elapsed: Duration,
}

pub struct RolaMiniControl {
    session: MiniSession,
    sidecar: NativeRtmSidecar,
}

impl RolaMiniControl {
    pub async fn connect(
        client: &EnabotClient,
        sidecar_path: &Path,
        session: MiniSession,
    ) -> Result<Self> {
        if !session.is_online {
            bail!("robot is not online according to Mini session endpoint");
        }

        let mut sidecar = NativeRtmSidecar::start(sidecar_path).await?;
        sidecar
            .connect(
                &client.config().agora_app_id,
                &session.app_rtm_uid,
                &session.app_rtm_token,
            )
            .await
            .context("native RTM login failed")?;

        Ok(Self { session, sidecar })
    }

    pub fn session(&self) -> &MiniSession {
        &self.session
    }

    pub async fn enter_live(&mut self) -> Result<()> {
        let payload = commands::enter_live(&self.session)?;
        self.send_payload(payload).await
    }

    pub async fn heartbeat(&mut self) -> Result<()> {
        let payload = commands::heartbeat(&self.session);
        self.send_payload(payload).await
    }

    pub async fn wait_for_live_control_ready(
        &mut self,
        ready_timeout: Duration,
        min_settle: Duration,
    ) -> Result<LiveReadyStatus> {
        let started = tokio::time::Instant::now();
        let ready_event = self
            .sidecar
            .collect_until(ready_timeout, |event| {
                live_ready_event_id(event, &self.session).is_some()
            })
            .await?;
        let event_id = ready_event
            .as_ref()
            .and_then(|event| live_ready_event_id(event, &self.session));

        let elapsed = started.elapsed();
        if elapsed < min_settle {
            tokio::time::sleep(min_settle - elapsed).await;
        }

        Ok(LiveReadyStatus {
            confirmed: event_id.is_some(),
            event_id,
            elapsed: started.elapsed(),
        })
    }

    pub async fn drive(&mut self, ly: i64, rx: i64) -> Result<()> {
        let payload = commands::drive(&self.session, ly, rx);
        self.send_payload(payload).await
    }

    pub async fn stop(&mut self) -> Result<()> {
        let payload = commands::stop(&self.session);
        self.send_payload(payload).await
    }

    pub async fn snapshot_trigger(&mut self) -> Result<()> {
        let payload = commands::snapshot();
        self.send_payload(payload).await
    }

    pub async fn set_video_quality(&mut self, quality: VideoQuality) -> Result<()> {
        let payload = commands::video_quality(&self.session, quality);
        self.send_payload(payload).await
    }

    pub async fn drive_for(&mut self, ly: i64, rx: i64, duration: Duration) -> Result<()> {
        let started = tokio::time::Instant::now();
        let frame_interval = Duration::from_millis(DRIVE_FRAME_INTERVAL_MS);
        self.drive(ly, rx).await?;

        loop {
            let elapsed = started.elapsed();
            if elapsed >= duration {
                break;
            }

            tokio::time::sleep((duration - elapsed).min(frame_interval)).await;
            if started.elapsed() < duration {
                self.drive(ly, rx).await?;
            }
        }

        self.stop().await
    }

    pub async fn collect_for(&mut self, duration: Duration) -> Result<()> {
        self.sidecar.collect_for(duration).await
    }

    pub fn take_events(&mut self) -> Vec<Value> {
        self.sidecar.take_events()
    }

    pub async fn logout(&mut self) -> Result<()> {
        self.sidecar.logout().await
    }

    async fn send_payload(&mut self, payload: Value) -> Result<()> {
        let message = serde_json::to_string(&payload)?;
        let mut last_error = None;

        for attempt in 0..8 {
            match self
                .sidecar
                .send_user_message(&self.session.mini_rtm_uid, &message)
                .await
            {
                Ok(()) => return Ok(()),
                Err(err) if retryable_send_error(&err) && attempt < 7 => {
                    last_error = Some(err);
                    let _ = self.sidecar.collect_for(Duration::from_millis(750)).await;
                    tokio::time::sleep(Duration::from_millis(750)).await;
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_error.expect("retry loop must store an error"))
    }
}

fn retryable_send_error(err: &anyhow::Error) -> bool {
    let text = format!("{err:#}").to_ascii_lowercase();
    text.contains("-11033") || text.contains("offline") || text.contains("request timed out")
}

fn live_ready_event_id(event: &Value, session: &MiniSession) -> Option<i64> {
    if event.get("event").and_then(Value::as_str) != Some("message") {
        return None;
    }
    if event.get("publisher").and_then(Value::as_str) != Some(session.mini_rtm_uid.as_str()) {
        return None;
    }

    let message = event.get("message").and_then(Value::as_str)?;
    let payload: Value = serde_json::from_str(message).ok()?;
    if let Some(sid) = payload
        .get("sid")
        .or_else(|| payload.get("rsid"))
        .and_then(Value::as_str)
    {
        if sid != session.sid {
            return None;
        }
    }

    let id = payload.get("id").and_then(Value::as_i64)?;
    match id {
        101004 | 101006 => Some(id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> MiniSession {
        MiniSession {
            sid: "session-id".to_string(),
            is_online: true,
            app_rtm_uid: "app-rtm".to_string(),
            app_rtm_token: "app-rtm-token".to_string(),
            app_rtc_uid: "100000001".to_string(),
            app_rtc_token: "app-rtc-token".to_string(),
            mini_rtm_uid: "mini-rtm".to_string(),
            mini_rtc_uid: "200000001".to_string(),
            rtc_channel: "mini-channel".to_string(),
        }
    }

    #[test]
    fn live_ready_event_accepts_robot_state_ack_for_session() {
        let session = session();
        let event = json!({
            "type": "event",
            "event": "message",
            "publisher": "mini-rtm",
            "message": r#"{"id":101006,"rsid":"session-id","type":0}"#,
        });

        assert_eq!(live_ready_event_id(&event, &session), Some(101006));
    }

    #[test]
    fn live_ready_event_rejects_other_sessions_and_peer_messages() {
        let session = session();
        let other_session = json!({
            "type": "event",
            "event": "message",
            "publisher": "mini-rtm",
            "message": r#"{"id":101006,"rsid":"old-session","type":0}"#,
        });
        let other_peer = json!({
            "type": "event",
            "event": "message",
            "publisher": "someone-else",
            "message": r#"{"id":101006,"sid":"session-id","type":0}"#,
        });

        assert_eq!(live_ready_event_id(&other_session, &session), None);
        assert_eq!(live_ready_event_id(&other_peer, &session), None);
    }
}
