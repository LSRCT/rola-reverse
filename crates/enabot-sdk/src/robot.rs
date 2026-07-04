use crate::client::{EnabotClient, MiniSession};
use crate::commands;
use crate::sidecar::NativeRtmSidecar;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

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

    pub async fn drive_for(&mut self, ly: i64, rx: i64, duration: Duration) -> Result<()> {
        self.drive(ly, rx).await?;
        tokio::time::sleep(duration).await;
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
    text.contains("-11033") || text.contains("offline")
}
