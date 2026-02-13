use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};

use breakpoint_core::events::Event;

use crate::state::AppState;

/// Request body for posting a single event.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PostEventsBody {
    Single(Box<Event>),
    Batch(Vec<Event>),
}

/// Response for a successful event post.
#[derive(Debug, Serialize)]
pub struct PostEventsResponse {
    pub accepted: usize,
    pub event_ids: Vec<String>,
}

/// POST /api/v1/events — accept single or batch events.
pub async fn post_events(
    State(state): State<AppState>,
    Json(body): Json<PostEventsBody>,
) -> Result<(StatusCode, Json<PostEventsResponse>), (StatusCode, String)> {
    let events = match body {
        PostEventsBody::Single(e) => vec![*e],
        PostEventsBody::Batch(v) => v,
    };

    if events.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No events provided".to_string()));
    }

    let mut event_ids = Vec::with_capacity(events.len());
    let mut store = state.event_store.write().await;
    for event in events {
        event_ids.push(event.id.clone());
        store.insert(event);
    }

    Ok((
        StatusCode::CREATED,
        Json(PostEventsResponse {
            accepted: event_ids.len(),
            event_ids,
        }),
    ))
}

/// Request body for claiming an event.
#[derive(Debug, Deserialize)]
pub struct ClaimEventBody {
    pub claimed_by: String,
}

/// Response for a successful event claim.
#[derive(Debug, Serialize)]
pub struct ClaimEventResponse {
    pub claimed: bool,
    pub event_id: String,
}

/// POST /api/v1/events/:event_id/claim — claim an event.
pub async fn claim_event(
    State(state): State<AppState>,
    axum::extract::Path(event_id): axum::extract::Path<String>,
    Json(body): Json<ClaimEventBody>,
) -> Result<Json<ClaimEventResponse>, (StatusCode, String)> {
    let mut store = state.event_store.write().await;
    let now = breakpoint_core::time::timestamp_now();
    let claimed = store.claim(&event_id, body.claimed_by, now);
    if claimed {
        Ok(Json(ClaimEventResponse {
            claimed: true,
            event_id,
        }))
    } else {
        Err((StatusCode::NOT_FOUND, format!("Event {event_id} not found")))
    }
}

/// Status response.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub stats: crate::event_store::EventStoreStats,
    pub recent_events: Vec<EventSummary>,
    pub pending_actions: Vec<EventSummary>,
}

/// Summary of an event for the status endpoint.
#[derive(Debug, Serialize)]
pub struct EventSummary {
    pub id: String,
    pub event_type: String,
    pub title: String,
    pub source: String,
    pub claimed_by: Option<String>,
}

/// GET /api/v1/status — returns pending actions, recent events, stats.
pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let store = state.event_store.read().await;
    let stats = store.stats();

    let recent_events: Vec<EventSummary> = store
        .recent(20)
        .into_iter()
        .map(|se| EventSummary {
            id: se.event.id.clone(),
            event_type: serde_json::to_string(&se.event.event_type).unwrap_or_default(),
            title: se.event.title.clone(),
            source: se.event.source.clone(),
            claimed_by: se.claimed_by.clone(),
        })
        .collect();

    let pending_actions: Vec<EventSummary> = store
        .pending_actions()
        .into_iter()
        .map(|se| EventSummary {
            id: se.event.id.clone(),
            event_type: serde_json::to_string(&se.event.event_type).unwrap_or_default(),
            title: se.event.title.clone(),
            source: se.event.source.clone(),
            claimed_by: se.claimed_by.clone(),
        })
        .collect();

    Json(StatusResponse {
        stats,
        recent_events,
        pending_actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use breakpoint_core::events::{EventType, Priority};
    use std::collections::HashMap;

    fn make_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            event_type: EventType::PipelineFailed,
            source: "github".to_string(),
            priority: Priority::Notice,
            title: "CI failed".to_string(),
            body: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            url: None,
            actor: None,
            tags: vec![],
            action_required: false,
            group_key: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn post_single_event() {
        let state = AppState::new(ServerConfig::default());
        let body = Json(PostEventsBody::Single(Box::new(make_event("evt-1"))));
        let result = post_events(State(state.clone()), body).await;
        assert!(result.is_ok());
        let (status, json) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json.accepted, 1);
        assert_eq!(json.event_ids, vec!["evt-1"]);

        let store = state.event_store.read().await;
        assert!(store.get("evt-1").is_some());
    }

    #[tokio::test]
    async fn post_batch_events() {
        let state = AppState::new(ServerConfig::default());
        let body = Json(PostEventsBody::Batch(vec![
            make_event("evt-1"),
            make_event("evt-2"),
        ]));
        let result = post_events(State(state), body).await;
        assert!(result.is_ok());
        let (_, json) = result.unwrap();
        assert_eq!(json.accepted, 2);
    }

    #[tokio::test]
    async fn post_empty_batch_fails() {
        let state = AppState::new(ServerConfig::default());
        let body = Json(PostEventsBody::Batch(vec![]));
        let result = post_events(State(state), body).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn claim_event_works() {
        let state = AppState::new(ServerConfig::default());
        {
            let mut store = state.event_store.write().await;
            store.insert(make_event("evt-1"));
        }

        let body = Json(ClaimEventBody {
            claimed_by: "alice".to_string(),
        });
        let path = axum::extract::Path("evt-1".to_string());
        let result = claim_event(State(state.clone()), path, body).await;
        assert!(result.is_ok());

        let store = state.event_store.read().await;
        assert_eq!(
            store.get("evt-1").unwrap().claimed_by.as_deref(),
            Some("alice")
        );
    }

    #[tokio::test]
    async fn claim_nonexistent_event_fails() {
        let state = AppState::new(ServerConfig::default());
        let body = Json(ClaimEventBody {
            claimed_by: "alice".to_string(),
        });
        let path = axum::extract::Path("nonexistent".to_string());
        let result = claim_event(State(state), path, body).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn status_endpoint() {
        let state = AppState::new(ServerConfig::default());
        {
            let mut store = state.event_store.write().await;
            store.insert(make_event("evt-1"));
            let mut e2 = make_event("evt-2");
            e2.action_required = true;
            store.insert(e2);
        }

        let json = get_status(State(state)).await;
        assert_eq!(json.stats.total_stored, 2);
        assert_eq!(json.stats.total_pending_actions, 1);
        assert_eq!(json.recent_events.len(), 2);
        assert_eq!(json.pending_actions.len(), 1);
    }
}
