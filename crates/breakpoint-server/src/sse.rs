use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::stream::Stream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::{AppState, ConnectionGuard};

/// GET /api/v1/events/stream — SSE endpoint for real-time event streaming.
pub async fn event_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, StatusCode> {
    let max_sse = state.config.limits.max_sse_subscribers;
    let current = state.sse_subscriber_count.load(Ordering::Relaxed);
    if current >= max_sse {
        tracing::warn!(current, max = max_sse, "SSE subscriber limit reached");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let guard = ConnectionGuard::new(Arc::clone(&state.sse_subscriber_count));

    let store = state.event_store.read().await;
    let rx = store.subscribe();
    drop(store);

    let stream = BroadcastStream::new(rx).filter_map(
        move |result: Result<breakpoint_core::events::Event, _>| {
            let _guard = &guard;
            match result {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    Some(Ok(SseEvent::default()
                        .event("alert")
                        .data(json)
                        .id(event.id.clone())))
                },
                Err(e) => {
                    tracing::warn!("SSE broadcast receive error: {e}");
                    None
                },
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
