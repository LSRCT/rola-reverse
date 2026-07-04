use aes_gcm::{
    AesGcm, KeyInit,
    aead::{AeadInPlace, OsRng, consts::U16, generic_array::GenericArray},
    aes::Aes128,
};
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, Mac as HmacMac};
use rand::{Rng, RngCore, distributions::Alphanumeric};
use serde::Serialize;
use serde_json::{Value, json};
use sha1::Sha1;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;
type Aes128Gcm16 = AesGcm<Aes128, U16>;

const LOGIN_PATH: &str = "/api/v1/s1/users/login/";

pub fn login_path() -> &'static str {
    LOGIN_PATH
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}

pub fn build_login_envelope<T: Serialize>(
    payload: &T,
    body_encrypt_key_s2: &str,
    access_key_secret_s2: &str,
) -> Result<Value> {
    let encrypted = encrypt_body(payload, body_encrypt_key_s2)?;
    let mut envelope = BTreeMap::new();
    envelope.insert("data".to_string(), Value::String(encrypted));
    envelope.insert("app_type".to_string(), json!(2));
    envelope.insert("e_ver".to_string(), json!("1.0"));
    envelope.insert("nonce".to_string(), json!(random_nonce(11)));
    envelope.insert("timestamp".to_string(), json!(now_ms()));
    envelope.insert("signature_version".to_string(), json!("1.1"));
    envelope.insert("signature_method".to_string(), json!("SHA1"));

    let sign = sign("POST", LOGIN_PATH, &envelope, access_key_secret_s2)?;
    envelope.insert("sign".to_string(), Value::String(sign));

    Ok(Value::Object(envelope.into_iter().collect()))
}

fn random_nonce(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn encrypt_body<T: Serialize>(payload: &T, key: &str) -> Result<String> {
    let key = key.as_bytes();
    if key.len() != 16 {
        bail!(
            "ENABOT_BODY_ENCRYPT_KEY_S2 must be 16 bytes, got {}",
            key.len()
        );
    }

    let mut iv = [0_u8; 16];
    OsRng.fill_bytes(&mut iv);

    let cipher = Aes128Gcm16::new_from_slice(key).map_err(|_| anyhow!("invalid AES key"))?;
    let nonce = GenericArray::from_slice(&iv);
    let mut buffer = serde_json::to_vec(payload).context("serializing encrypted login payload")?;
    let tag = cipher
        .encrypt_in_place_detached(nonce, b"", &mut buffer)
        .map_err(|_| anyhow!("AES-GCM encryption failed"))?;

    let mut out = Vec::with_capacity(iv.len() + buffer.len() + tag.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&buffer);
    out.extend_from_slice(&tag);
    Ok(general_purpose::STANDARD.encode(out))
}

fn sign(
    method: &str,
    relative_path: &str,
    params: &BTreeMap<String, Value>,
    secret: &str,
) -> Result<String> {
    let canonical = canonical_params(params);
    let sign_text = format!(
        "{}&{}&{}",
        method.to_uppercase(),
        java_url_encode(relative_path),
        java_url_encode(&canonical)
    );
    let mut mac = <HmacSha1 as HmacMac>::new_from_slice(secret.as_bytes())
        .map_err(|_| anyhow!("invalid HMAC secret"))?;
    mac.update(sign_text.as_bytes());
    Ok(general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

fn canonical_params(params: &BTreeMap<String, Value>) -> String {
    params
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            format!("{}={}", java_url_encode(key), java_url_encode(&value))
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn java_url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
