//! Remote notification channels — Telegram, DingTalk, WeChat push.

use base64::Engine as _;
use crate::config::{DingTalkConfig, TelegramConfig, WeChatConfig};

/// Send a message to Telegram bot.
pub async fn send_telegram(config: &TelegramConfig, message: &str) {
    if !config.enabled || config.bot_token.is_empty() || config.chat_id.is_empty() {
        return;
    }

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.bot_token
    );
    let client = reqwest::Client::new();
    let res = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": config.chat_id,
            "text": message,
        }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    if let Err(e) = res {
        tracing::warn!("Telegram send error: {}", e);
    }
}

/// Send a message to DingTalk webhook.
pub async fn send_dingtalk(config: &DingTalkConfig, message: &str) {
    if !config.enabled || config.access_token.is_empty() {
        return;
    }

    let webhook = if config.webhook_url.is_empty() {
        "https://oapi.dingtalk.com/robot/send"
    } else {
        &config.webhook_url
    };

    let timestamp = chrono::Utc::now().timestamp_millis().to_string();

    let mut sign = String::new();
    if !config.secret.is_empty() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let string_to_sign = format!("{}\n{}", timestamp, config.secret);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(config.secret.as_bytes()).expect("HMAC key");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize().into_bytes();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&result);
        sign = urlencoding::encode(&b64).into_owned();
    }

    let url = format!(
        "{}?access_token={}&timestamp={}&sign={}",
        webhook, config.access_token, timestamp, sign
    );

    let client = reqwest::Client::new();
    let res = client
        .post(&url)
        .json(&serde_json::json!({
            "msgtype": "text",
            "text": { "content": message }
        }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    if let Err(e) = res {
        tracing::warn!("DingTalk send error: {}", e);
    }
}

/// Send a message to WeChat (PushPlus or ServerChan).
pub async fn send_wechat(config: &WeChatConfig, message: &str) {
    if !config.enabled {
        return;
    }

    let client = reqwest::Client::new();
    let provider = if config.provider.is_empty() {
        "pushplus"
    } else {
        &config.provider
    };

    let res = match provider {
        "pushplus" => {
            if config.pushplus_token.is_empty() {
                return;
            }
            client
                .post("https://www.pushplus.plus/send")
                .json(&serde_json::json!({
                    "token": config.pushplus_token,
                    "title": "Agent Desk",
                    "content": message,
                }))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
        }
        "serverchan" => {
            if config.serverchan_sendkey.is_empty() {
                return;
            }
            let url = format!(
                "https://sctapi.ftqq.com/{}.send",
                config.serverchan_sendkey
            );
            client
                .post(&url)
                .json(&serde_json::json!({
                    "title": "Agent Desk",
                    "desp": message,
                }))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
        }
        _ => return,
    };

    if let Err(e) = res {
        tracing::warn!("WeChat ({}) send error: {}", provider, e);
    }
}

/// Dispatch message to all enabled remote channels concurrently.
pub async fn dispatch_remote(
    telegram: &TelegramConfig,
    dingtalk: &DingTalkConfig,
    wechat: &WeChatConfig,
    message: &str,
) {
    tokio::join!(
        send_telegram(telegram, message),
        send_dingtalk(dingtalk, message),
        send_wechat(wechat, message),
    );
}
