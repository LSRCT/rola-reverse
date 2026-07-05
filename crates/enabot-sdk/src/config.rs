use anyhow::{Context, Result, bail};
use rand::{RngCore, rngs::OsRng};
use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

const LOCAL_STATE_DIR: &str = ".enabot";
const DEVICE_ID_FILE: &str = ".enabot/device_id";

#[derive(Debug, Clone)]
pub struct Config {
    pub account: String,
    pub password: String,
    pub device_id: String,
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

        let device_id = optional(&values, "ENABOT_DEVICE_ID", "");
        let device_id = if device_id.is_empty() {
            load_or_create_device_id(Path::new(DEVICE_ID_FILE))?
        } else {
            device_id
        };

        Ok(Self {
            account: need(&values, "ENABOT_ACCOUNT")?,
            password: need(&values, "ENABOT_PASSWORD")?,
            device_id,
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

fn load_or_create_device_id(path: &Path) -> Result<String> {
    if path.exists() {
        let value = fs::read_to_string(path)
            .with_context(|| format!("reading generated device id from {}", path.display()))?;
        let value = value.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
    }

    fs::create_dir_all(LOCAL_STATE_DIR).context("creating local Enabot state directory")?;
    let value = generate_device_id();
    fs::write(path, format!("{value}\n"))
        .with_context(|| format!("writing generated device id to {}", path.display()))?;
    Ok(value)
}

fn generate_device_id() -> String {
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);

    let mut out = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
