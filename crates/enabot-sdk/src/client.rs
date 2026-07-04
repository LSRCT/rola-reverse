use crate::auth::build_login_envelope;
use crate::config::Config;
use anyhow::{Context, Result, bail};
use reqwest::header::{ACCEPT_LANGUAGE, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, SET_COOKIE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const HOST: &str = "ebo.enabotserverintl.com";
const LOGIN_URL: &str = "https://ebo.enabotserverintl.com/api/v1/s1/users/login/";
const MINI_SESSION_URL: &str = "https://ebo.enabotserverintl.com/api/v1/rola/mini/session";
const ROBOTS_URL: &str = "https://ebo.enabotserverintl.com/api/v1/robots/robot/";
const USERS_DETAILS_URL: &str = "https://ebo.enabotserverintl.com/api/v1/users/details/";

#[derive(Debug, Clone)]
pub struct EnabotClient {
    http: reqwest::Client,
    config: Config,
}

#[derive(Debug, Clone)]
pub struct LoginSession {
    pub cookie_header: String,
    pub csrf_token: String,
    pub cookie_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MiniSession {
    pub sid: String,
    pub app_rtc_uid: String,
    pub app_rtc_token: String,
    pub app_rtm_uid: String,
    pub app_rtm_token: String,
    pub rtc_channel: String,
    pub mini_rtc_uid: String,
    pub mini_rtm_uid: String,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RobotInfo {
    pub robot_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_online: Option<bool>,
}

#[derive(Debug, Serialize)]
struct LoginPayload<'a> {
    app_kind: &'static str,
    app_token: &'a str,
    device_id: &'a str,
    language: &'a str,
    account: &'a str,
    password: &'a str,
    login_region: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    phone_area: &'a str,
}

#[derive(Debug, Serialize)]
struct MiniSessionRequest {
    require_online_status: bool,
    robot_id: u64,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i64,
    msg: Option<String>,
    data: Option<T>,
}

impl EnabotClient {
    pub fn new(config: Config) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("okhttp/4.12.0")
            .build()
            .context("building HTTP client")?;
        Ok(Self { http, config })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn login(&self) -> Result<LoginSession> {
        let payload = LoginPayload {
            app_kind: "Android",
            app_token: &self.config.app_token,
            device_id: &self.config.device_id,
            language: &self.config.language,
            account: &self.config.account,
            password: &self.config.password,
            login_region: &self.config.login_region,
            phone_area: &self.config.phone_area,
        };
        let envelope = build_login_envelope(
            &payload,
            &self.config.body_encrypt_key_s2,
            &self.config.access_key_secret_s2,
        )?;

        let response = self
            .http
            .post(LOGIN_URL)
            .header(ACCEPT_LANGUAGE, &self.config.accept_language)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .json(&envelope)
            .send()
            .await
            .context("sending Enabot login")?;

        let status = response.status();
        let headers = response.headers().clone();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("login failed: HTTP {status}: {text}");
        }

        let set_cookies = set_cookie_headers(&headers)?;
        let cookie_names = set_cookies
            .iter()
            .filter_map(|cookie| cookie.split_once('=').map(|(name, _)| name.to_string()))
            .collect::<Vec<_>>();
        let cookie_header = set_cookies
            .iter()
            .filter_map(|cookie| cookie.split_once(';').map(|(first, _)| first.to_string()))
            .collect::<Vec<_>>()
            .join("; ");
        let csrf_token = set_cookies
            .iter()
            .find_map(|cookie| {
                cookie
                    .strip_prefix("csrftoken=")
                    .and_then(|rest| rest.split_once(';').map(|(token, _)| token.to_string()))
            })
            .unwrap_or_default();

        if cookie_header.is_empty() {
            bail!("login response did not set cookies for {HOST}");
        }
        if csrf_token.is_empty() {
            bail!("login response did not set csrftoken");
        }

        Ok(LoginSession {
            cookie_header,
            csrf_token,
            cookie_names,
        })
    }

    pub async fn mini_session(&self, login: &LoginSession) -> Result<MiniSession> {
        let robot_id = self
            .config
            .robot_id
            .context("missing ENABOT_ROBOT_ID; run `enabot robots` to list account-bound robots")?;
        let body = MiniSessionRequest {
            require_online_status: true,
            robot_id,
        };

        let response = self
            .http
            .post(MINI_SESSION_URL)
            .header(ACCEPT_LANGUAGE, &self.config.accept_language)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .header(COOKIE, HeaderValue::from_str(&login.cookie_header)?)
            .header("X-CSRFToken", &login.csrf_token)
            .json(&body)
            .send()
            .await
            .context("requesting Mini session")?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("reading Mini session response")?;
        if !status.is_success() {
            bail!("Mini session failed: HTTP {status}: {text}");
        }

        let parsed: ApiResponse<MiniSession> =
            serde_json::from_str(&text).context("parsing Mini session response")?;
        if parsed.code != 200 {
            bail!("Mini session API error {}: {:?}", parsed.code, parsed.msg);
        }
        parsed.data.context("Mini session response missing data")
    }

    pub async fn robots(&self, login: &LoginSession) -> Result<Vec<RobotInfo>> {
        let mut robots = Vec::new();
        let mut errors = Vec::new();

        for url in [ROBOTS_URL, USERS_DETAILS_URL] {
            match self.get_api_data(login, url).await {
                Ok(data) => collect_robots(&data, &mut robots),
                Err(err) => errors.push(format!("{url}: {err:#}")),
            }
        }

        let mut seen = HashSet::new();
        robots.retain(|robot| seen.insert(robot.robot_id));
        robots.sort_by_key(|robot| robot.robot_id);

        if robots.is_empty() && !errors.is_empty() {
            bail!("could not load robots: {}", errors.join("; "));
        }

        Ok(robots)
    }

    async fn get_api_data(&self, login: &LoginSession, url: &str) -> Result<Value> {
        let response = self
            .http
            .get(url)
            .header(ACCEPT_LANGUAGE, &self.config.accept_language)
            .header(COOKIE, HeaderValue::from_str(&login.cookie_header)?)
            .header("X-CSRFToken", &login.csrf_token)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .with_context(|| format!("reading {url} response"))?;
        if !status.is_success() {
            bail!("{url} failed: HTTP {status}: {text}");
        }

        let parsed: ApiResponse<Value> =
            serde_json::from_str(&text).with_context(|| format!("parsing {url} response"))?;
        if !matches!(parsed.code, 200 | 194400) {
            bail!("{url} API error {}: {:?}", parsed.code, parsed.msg);
        }
        parsed.data.with_context(|| format!("{url} missing data"))
    }
}

fn set_cookie_headers(headers: &HeaderMap) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for value in headers.get_all(SET_COOKIE) {
        out.push(value.to_str()?.to_string());
    }
    Ok(out)
}

fn collect_robots(value: &Value, out: &mut Vec<RobotInfo>) {
    if let Some(robot) = robot_from_value(value) {
        out.push(robot);
    }

    match value {
        Value::Array(items) => {
            for item in items {
                collect_robots(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_robots(item, out);
            }
        }
        _ => {}
    }
}

fn robot_from_value(value: &Value) -> Option<RobotInfo> {
    let object = value.as_object()?;
    let robot_id = value_as_u64(object.get("robot_id").or_else(|| object.get("robotId"))?)?;
    Some(RobotInfo {
        robot_id,
        name: first_string(
            object,
            &[
                "robot_name",
                "robotName",
                "name",
                "nickname",
                "alias",
                "device_name",
            ],
        ),
        model: first_string(
            object,
            &[
                "machine_version",
                "model_marketing_name",
                "product_name",
                "model",
                "machine_type",
            ],
        ),
        is_online: first_bool(
            object,
            &["is_online", "isOnline", "online", "online_status"],
        ),
    })
}

fn first_string(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        object
            .get(*name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn first_bool(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<bool> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(value_as_bool))
}

fn value_as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn value_as_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value.as_i64().map(|value| value != 0).or_else(|| {
            value.as_str().and_then(|value| match value {
                "1" | "true" | "True" | "TRUE" => Some(true),
                "0" | "false" | "False" | "FALSE" => Some(false),
                _ => None,
            })
        })
    })
}
