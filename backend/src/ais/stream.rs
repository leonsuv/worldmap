//! AISstream.io WebSocket client with reconnect and status reporting.

use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

use super::{parse_message, AisHub, Update};

const STREAM_URL: &str = "wss://stream.aisstream.io/v0/stream";

/// The stream endpoint; `AISSTREAM_URL` points it at a recorded or simulated feed.
fn stream_url() -> String {
    std::env::var("AISSTREAM_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| STREAM_URL.to_string())
}
/// Reconnect when the stream has been silent for this long.
const READ_TIMEOUT: Duration = Duration::from_secs(90);

enum Outcome {
    Closed,
    Rejected(String),
}

pub fn spawn(api_key: String, hub: Arc<AisHub>) {
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            let started = Instant::now();
            let result = connect_and_stream(&api_key, &hub).await;
            hub.update_status(|s| {
                s.connected = false;
                s.connected_since = None;
            });
            let delay = match result {
                Ok(Outcome::Closed) => {
                    tracing::info!("AIS stream closed; reconnecting");
                    Duration::from_secs(1)
                }
                Ok(Outcome::Rejected(reason)) => {
                    tracing::error!("AISstream rejected the subscription: {reason}. Check AISSTREAM_API_KEY.");
                    hub.update_status(|s| s.last_error = Some(reason));
                    Duration::from_secs(300)
                }
                Err(e) => {
                    tracing::warn!("AIS stream error: {e:#}");
                    hub.update_status(|s| s.last_error = Some(e.to_string()));
                    backoff
                }
            };
            // A session that lasted a while resets the exponential backoff.
            backoff = if started.elapsed() > Duration::from_secs(60) {
                Duration::from_secs(1)
            } else {
                (backoff * 2).min(Duration::from_secs(60))
            };
            tokio::time::sleep(delay).await;
        }
    });
}

async fn connect_and_stream(api_key: &str, hub: &AisHub) -> anyhow::Result<Outcome> {
    let (ws, _) = tokio::time::timeout(Duration::from_secs(20), tokio_tungstenite::connect_async(stream_url())).await??;
    let (mut write, mut read) = ws.split();

    let subscription = serde_json::json!({
        "APIKey": api_key,
        "BoundingBoxes": [[[-90, -180], [90, 180]]],
        "FilterMessageTypes": [
            "PositionReport",
            "StandardClassBPositionReport",
            "ExtendedClassBPositionReport",
            "LongRangeAisBroadcastMessage",
            "ShipStaticData",
            "StaticDataReport",
            "AidsToNavigationReport",
            "StandardSearchAndRescueAircraftReport"
        ]
    });
    write.send(Message::Text(subscription.to_string().into())).await?;
    tracing::info!("Connected to AISstream");

    let mut first = true;
    loop {
        let next = tokio::time::timeout(READ_TIMEOUT, read.next())
            .await
            .map_err(|_| anyhow::anyhow!("no AIS data for {}s", READ_TIMEOUT.as_secs()))?;
        let Some(message) = next else { return Ok(Outcome::Closed) };
        let text = match message? {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Message::Ping(payload) => {
                write.send(Message::Pong(payload)).await?;
                continue;
            }
            Message::Close(frame) => {
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                if first && !reason.is_empty() {
                    return Ok(Outcome::Rejected(reason));
                }
                return Ok(Outcome::Closed);
            }
            _ => continue,
        };

        let Some(update) = parse_message(&text) else { continue };
        if let Update::Error(reason) = update {
            return Ok(Outcome::Rejected(reason));
        }
        let now = chrono::Utc::now().timestamp();
        if first {
            first = false;
            hub.update_status(|s| {
                s.connected = true;
                s.connected_since = Some(now);
                s.last_error = None;
            });
        }
        hub.update_status(|s| {
            s.messages += 1;
            s.last_message_at = Some(now);
        });
        hub.apply(update);
    }
}
