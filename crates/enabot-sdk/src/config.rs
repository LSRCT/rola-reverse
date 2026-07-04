use anyhow::{Context, Result, bail};
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone)]
pub struct Config {
    pub account: String,
    pub password: String,
    pub device_id: String,
    pub robot_id: u64,
    pub app_token: String,
    pub access_key_secret_s2: String,
    pub body_encrypt_key_s2: String,
    pub agora_app_id: String,
    pub login_region: String,
    pub phone_area: String,
    pub language: String,
    pub accept_language: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut values = HashMap::new();
        load_env_file(Path::new(".env"), &mut values)?;
        for (key, value) in std::env::vars() {
            values.insert(key, value);
        }

        let robot_id = need(&values, "ENABOT_ROBOT_ID")?
            .parse()
            .context("ENABOT_ROBOT_ID must be an integer")?;

        Ok(Self {
            account: need(&values, "ENABOT_ACCOUNT")?,
            password: need(&values, "ENABOT_PASSWORD")?,
            device_id: need(&values, "ENABOT_DEVICE_ID")?,
            robot_id,
            app_token: need(&values, "ENABOT_APP_TOKEN")?,
            access_key_secret_s2: need(&values, "ENABOT_ACCESS_KEY_SECRET_S2")?,
            body_encrypt_key_s2: need(&values, "ENABOT_BODY_ENCRYPT_KEY_S2")?,
            agora_app_id: need(&values, "AGORA_APP_ID")?,
            login_region: optional(&values, "ENABOT_LOGIN_REGION", "GB"),
            phone_area: optional(&values, "ENABOT_PHONE_AREA", ""),
            language: optional(&values, "ENABOT_LANGUAGE", "en"),
            accept_language: optional(&values, "ENABOT_ACCEPT_LANGUAGE", "en"),
        })
    }
}

fn need(values: &HashMap<String, String>, name: &str) -> Result<String> {
    match values.get(name).filter(|value| !value.is_empty()) {
        Some(value) => Ok(value.clone()),
        None => bail!("missing required env {name}"),
    }
}

fn optional(values: &HashMap<String, String>, name: &str, fallback: &str) -> String {
    values
        .get(name)
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn load_env_file(path: &Path, values: &mut HashMap<String, String>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(name.trim().to_string(), parse_env_value(value.trim())?);
    }
    Ok(())
}

fn parse_env_value(value: &str) -> Result<String> {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return serde_json::from_str(value).context("parsing quoted .env value");
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return Ok(value[1..value.len() - 1].to_string());
    }
    Ok(value.to_string())
}
