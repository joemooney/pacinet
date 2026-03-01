//! Async webhook delivery for FSM alert actions.
//!
//! Fire-and-forget via tokio::spawn. Supports bearer token, basic auth,
//! custom headers, and retry with exponential backoff.

use crate::metrics as m;
use chrono::{DateTime, Utc};
use pacinet_core::fsm::WebhookConfig;
use serde::Serialize;
use tracing::{info, warn};

/// Payload sent to webhook endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub instance_id: String,
    pub definition_name: String,
    pub current_state: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub deployed_nodes: Vec<String>,
}

/// Deliver a webhook payload to the configured endpoint.
/// Retries with exponential backoff on failure.
/// Optionally records delivery history to storage.
pub async fn deliver_webhook(
    config: &WebhookConfig,
    payload: &WebhookPayload,
    storage: Option<&std::sync::Arc<dyn pacinet_core::Storage>>,
    instance_id: &str,
) {
    let max_retries = config.max_retries.unwrap_or(2);
    let timeout_secs = config.timeout_seconds.unwrap_or(10);
    let method = config.method.as_deref().unwrap_or("POST");

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(url = %config.url, error = %e, "Failed to build HTTP client for webhook");
            m::record_webhook_delivery("client_error");
            return;
        }
    };

    for attempt in 0..=max_retries {
        let start = tokio::time::Instant::now();
        let mut request = match method.to_uppercase().as_str() {
            "PUT" => client.put(&config.url),
            "PATCH" => client.patch(&config.url),
            _ => client.post(&config.url),
        };

        // Set auth headers
        if let Some(ref token) = config.bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some(ref auth) = config.basic_auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        // Custom headers
        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        request = request.json(payload);

        match request.send().await {
            Ok(resp) => {
                let status = resp.status();
                let duration_ms = start.elapsed().as_millis() as u64;
                if status.is_success() {
                    info!(
                        url = %config.url,
                        status = %status,
                        attempt = attempt,
                        "Webhook delivered successfully"
                    );
                    m::record_webhook_delivery("success");
                    record_delivery(
                        storage,
                        instance_id,
                        &config.url,
                        method,
                        Some(status.as_u16()),
                        true,
                        duration_ms,
                        None,
                        attempt,
                    );
                    return;
                }
                warn!(
                    url = %config.url,
                    status = %status,
                    attempt = attempt,
                    "Webhook returned non-success status"
                );
                record_delivery(
                    storage,
                    instance_id,
                    &config.url,
                    method,
                    Some(status.as_u16()),
                    false,
                    duration_ms,
                    Some(format!("HTTP {}", status)),
                    attempt,
                );
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                warn!(
                    url = %config.url,
                    error = %e,
                    attempt = attempt,
                    "Webhook delivery failed"
                );
                record_delivery(
                    storage,
                    instance_id,
                    &config.url,
                    method,
                    None,
                    false,
                    duration_ms,
                    Some(e.to_string()),
                    attempt,
                );
            }
        }

        if attempt < max_retries {
            let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }
    }

    warn!(
        url = %config.url,
        max_retries = max_retries,
        "Webhook delivery exhausted all retries"
    );
    m::record_webhook_delivery("exhausted");
}

#[allow(clippy::too_many_arguments)]
fn record_delivery(
    storage: Option<&std::sync::Arc<dyn pacinet_core::Storage>>,
    instance_id: &str,
    url: &str,
    method: &str,
    status_code: Option<u16>,
    success: bool,
    duration_ms: u64,
    error: Option<String>,
    attempt: u32,
) {
    if let Some(s) = storage {
        let delivery = pacinet_core::WebhookDelivery {
            id: uuid::Uuid::new_v4().to_string(),
            instance_id: instance_id.to_string(),
            url: url.to_string(),
            method: method.to_string(),
            status_code,
            success,
            duration_ms,
            error,
            attempt,
            timestamp: Utc::now(),
        };
        let storage = s.clone();
        tokio::task::spawn_blocking(move || {
            let _ = storage.store_webhook_delivery(delivery);
        });
    }
}
