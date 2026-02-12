use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::AppState;

/// GET /api/v1/events/stream — SSE endpoint for real-time event streaming.
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let store = state.event_store.read().await;
    let rx = store.subscribe();
    drop(store);

    let stream =
        BroadcastStream::new(rx).filter_map(|result: Result<breakpoint_core::events::Event, _>| {
            result.ok().map(|event| {
                let json = serde_json::to_string(&event).unwrap_or_default();
                Ok(SseEvent::default()
                    .event("alert")
                    .data(json)
                    .id(event.id.clone()))
            })
        });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
