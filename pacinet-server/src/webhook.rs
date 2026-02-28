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
pub async fn deliver_webhook(config: &WebhookConfig, payload: &WebhookPayload) {
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
                if status.is_success() {
                    info!(
                        url = %config.url,
                        status = %status,
                        attempt = attempt,
                        "Webhook delivered successfully"
                    );
                    m::record_webhook_delivery("success");
                    return;
                }
                warn!(
                    url = %config.url,
                    status = %status,
                    attempt = attempt,
                    "Webhook returned non-success status"
                );
            }
            Err(e) => {
                warn!(
                    url = %config.url,
                    error = %e,
                    attempt = attempt,
                    "Webhook delivery failed"
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
