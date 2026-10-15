//! Realtime WebSocket layer for Paid Rides and Driver Dispatch.
//!
//! Provides the in-memory DashMap registry mapping:
//! - Driver ID -> Internal Message Mailbox (tokio mpsc sender)
//! - Ride ID -> Broadcast Channel (for passenger & assigned driver)
//!
//! Implements the 4-Phase Lifecycle:
//! 1. Driver Goes Online (The Handshake):
//!    - WebSocket connection upgraded at `/rides/ws`.
//!    - Connection split into Incoming Listener (GPS fixes) and Outgoing Speaker (push alerts).
//!    - Driver's mailbox (mpsc::Sender) registered in-memory in DashMap.
//! 2. The 4-Second Location Heartbeat:
//!    - Driver sends GPS fix every 4 seconds.
//!    - Ingested directly into H3 spatial index in Redis via `dispatcher.ingest_driver_location` (IN MEMORY, NOT DB).
//!    - Refreshes 15-second dead-man's switch TTL.
//! 3. Rider Requests a Trip (Match & Dispatch):
//!    - Matches closest drivers from Redis H3 index.
//!    - Looks up driver ID in DashMap to grab their internal mailbox.
//!    - Pushes `driver.request.received` frame down open WebSocket.
//! 4. Driver Goes Offline (Cleanup):
//!    - Clean disconnect or sudden drop (TTL expiry):
//!    - Mailbox removed from DashMap; driver removed from Redis spatial map.

use crate::auth::jwt::Claims;
use crate::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Internal message pushed down to a connected client's WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutgoingWsMessage {
    /// Server frame: { "type": "event", "event": ... }
    #[serde(rename = "event")]
    Event { event: WsEventFrame },
    /// Heartbeat response
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsEventFrame {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ride_id: Option<Uuid>,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ride_version: Option<i32>,
    pub occurred_at: String,
    pub payload: serde_json::Value,
}

/// Incoming client frames over WebSocket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum IncomingWsFrame {
    #[serde(rename = "subscribe")]
    Subscribe { channel: String },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { channel: String },
    #[serde(rename = "location")]
    Location { location: crate::vrp::dtos::rides::LiveLocation },
    #[serde(rename = "ping")]
    Ping,
}

/// In-memory DashMap registry for live driver network sockets and ride broadcast channels.
#[derive(Clone, Default)]
pub struct WsRegistry {
    /// Driver ID -> Internal Message Mailbox (Outgoing Speaker sender)
    pub drivers: Arc<DashMap<Uuid, mpsc::Sender<OutgoingWsMessage>>>,
    /// Ride ID -> Broadcast Channel for active ride updates
    pub ride_channels: Arc<DashMap<Uuid, broadcast::Sender<WsEventFrame>>>,
    /// Driver ID -> Online availability in RAM
    pub driver_online: Arc<DashMap<Uuid, bool>>,
    /// Driver ID -> Current Active Ride ID in RAM
    pub driver_active_rides: Arc<DashMap<Uuid, Uuid>>,
}

impl WsRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(DashMap::new()),
            ride_channels: Arc::new(DashMap::new()),
            driver_online: Arc::new(DashMap::new()),
            driver_active_rides: Arc::new(DashMap::new()),
        }
    }

    /// Phase 1: Registers a driver's outgoing mailbox upon WebSocket handshake.
    pub fn register_driver(&self, driver_id: Uuid, tx: mpsc::Sender<OutgoingWsMessage>) {
        self.drivers.insert(driver_id, tx);
        tracing::info!("Registered driver in DashMap mailbox: {}", driver_id);
    }

    /// Sets driver online availability in memory.
    pub fn set_driver_online(&self, driver_id: Uuid, online: bool) {
        if online {
            self.driver_online.insert(driver_id, true);
        } else {
            self.driver_online.remove(&driver_id);
        }
    }

    /// Checks if driver is marked online in memory.
    pub fn is_driver_online(&self, driver_id: &Uuid) -> bool {
        self.driver_online.get(driver_id).map(|v| *v).unwrap_or(false)
    }

    /// Sets or clears the active ride for a driver in memory.
    pub fn set_driver_active_ride(&self, driver_id: Uuid, ride_id: Option<Uuid>) {
        if let Some(id) = ride_id {
            self.driver_active_rides.insert(driver_id, id);
        } else {
            self.driver_active_rides.remove(&driver_id);
        }
    }

    /// Returns the driver's current active ride ID if in memory.
    pub fn get_driver_active_ride(&self, driver_id: &Uuid) -> Option<Uuid> {
        self.driver_active_rides.get(driver_id).map(|v| *v)
    }

    /// Phase 4: Unregisters a driver's mailbox from RAM when disconnecting.
    pub fn unregister_driver(&self, driver_id: &Uuid) {
        if self.drivers.remove(driver_id).is_some() {
            tracing::info!("Unregistered driver from DashMap: {}", driver_id);
        }
    }

    /// Checks if a driver currently has an active WebSocket connection in RAM.
    pub fn is_driver_connected(&self, driver_id: &Uuid) -> bool {
        self.drivers.contains_key(driver_id)
    }

    /// Phase 3 (Step B & C): Find driver in DashMap and push the offer down their WebSocket.
    pub async fn send_to_driver(
        &self,
        driver_id: &Uuid,
        msg: OutgoingWsMessage,
    ) -> Result<(), &'static str> {
        if let Some(sender) = self.drivers.get(driver_id) {
            sender
                .send(msg)
                .await
                .map_err(|_| "Failed to send to driver mailbox")?;
            Ok(())
        } else {
            Err("Driver not connected in memory")
        }
    }

    /// Pushes a ride update event to all subscribers of `ride:<ride_id>`.
    pub fn broadcast_ride_event(&self, ride_id: Uuid, event: WsEventFrame) {
        if let Some(bcast) = self.ride_channels.get(&ride_id) {
            let _ = bcast.send(event);
        }
    }

    /// Gets or creates a broadcast channel for a ride.
    pub fn get_or_create_ride_channel(&self, ride_id: Uuid) -> broadcast::Sender<WsEventFrame> {
        self.ride_channels
            .entry(ride_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(100);
                tx
            })
            .clone()
    }
}

/// WebSocket route definition: GET /rides/ws
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// WebSocket Upgrade Handler. Authenticates via session cookie or Bearer token Claims.
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, claims, socket))
}

/// Handles the upgraded WebSocket connection across the 4 phases.
async fn handle_socket(state: Arc<AppState>, claims: Claims, socket: WebSocket) {
    let user_id = claims.user_id;
    let is_driver = claims.role == crate::users::RoleType::Driver;

    // Phase 1: Splitting the line into Incoming Listener and Outgoing Speaker
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create internal mailbox (tokio channel)
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingWsMessage>(64);

    // If user is a driver, register their mailbox in DashMap
    if is_driver {
        state.ws_registry.register_driver(user_id, outgoing_tx.clone());
    }

    // Task: Outgoing Speaker (sends messages down to the client's WebSocket)
    let send_task = tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            if let Ok(json_str) = serde_json::to_string(&msg) {
                if ws_sender.send(Message::Text(json_str.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task: Incoming Listener (receives 4-second GPS location heartbeats & pings)
    let app_state_clone = state.clone();
    let mut dispatcher = (*state.dispatcher).clone();
    let mut ride_subscriptions: std::collections::HashMap<Uuid, tokio::task::JoinHandle<()>> =
        std::collections::HashMap::new();

    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                if let Ok(frame) = serde_json::from_str::<IncomingWsFrame>(&text) {
                    match frame {
                        IncomingWsFrame::Ping => {
                            let _ = outgoing_tx.send(OutgoingWsMessage::Pong).await;
                        }
                        IncomingWsFrame::Location { location } => {
                            // Phase 2: 4-Second Location Heartbeat (IN-MEMORY & REDIS ONLY, NO DB QUERIES)
                            if is_driver {
                                // Only ingest into Redis spatial matching pool if driver is currently marked ONLINE in RAM
                                if app_state_clone.ws_registry.is_driver_online(&user_id) {
                                    let driver_state = dispatcher::DriverState {
                                        driver_id: user_id.to_string(),
                                        lat: location.point.lat,
                                        lng: location.point.lng,
                                        available_seats: 4,
                                    };

                                    let _ = dispatcher.ingest_driver_location(&driver_state).await;
                                }

                                // Broadcast location update if driver has an active ride in RAM
                                if let Some(ride_id) = app_state_clone.ws_registry.get_driver_active_ride(&user_id) {
                                    let loc_event = WsEventFrame {
                                        id: format!("evt_{}", Uuid::now_v7()),
                                        ride_id: Some(ride_id),
                                        seq: 1,
                                        ride_version: None,
                                        occurred_at: chrono::Utc::now().to_rfc3339(),
                                        payload: serde_json::json!({
                                            "type": "ride.driver.location_updated",
                                            "location": location,
                                        }),
                                    };
                                    app_state_clone
                                        .ws_registry
                                        .broadcast_ride_event(ride_id, loc_event);
                                }
                            }
                        }
                        IncomingWsFrame::Subscribe { channel } => {
                            tracing::debug!("User {} subscribed to {}", user_id, channel);
                            if channel.starts_with("ride:") {
                                if let Ok(ride_id) = Uuid::parse_str(&channel[5..]) {
                                    if is_driver {
                                        app_state_clone.ws_registry.set_driver_active_ride(user_id, Some(ride_id));
                                    }
                                    if !ride_subscriptions.contains_key(&ride_id) {
                                        let mut rx = app_state_clone
                                            .ws_registry
                                            .get_or_create_ride_channel(ride_id)
                                            .subscribe();
                                        let tx = outgoing_tx.clone();
                                        let sub_handle = tokio::spawn(async move {
                                            while let Ok(event) = rx.recv().await {
                                                if tx
                                                    .send(OutgoingWsMessage::Event { event })
                                                    .await
                                                    .is_err()
                                                {
                                                    break;
                                                }
                                            }
                                        });
                                        ride_subscriptions.insert(ride_id, sub_handle);
                                    }
                                }
                            }
                        }
                        IncomingWsFrame::Unsubscribe { channel } => {
                            tracing::debug!("User {} unsubscribed from {}", user_id, channel);
                            if channel.starts_with("ride:") {
                                if let Ok(ride_id) = Uuid::parse_str(&channel[5..]) {
                                    if is_driver && app_state_clone.ws_registry.get_driver_active_ride(&user_id) == Some(ride_id) {
                                        app_state_clone.ws_registry.set_driver_active_ride(user_id, None);
                                    }
                                    if let Some(handle) = ride_subscriptions.remove(&ride_id) {
                                        handle.abort();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    for (_, handle) in ride_subscriptions {
        handle.abort();
    }

    // Phase 4: Cleanup on disconnect
    if is_driver {
        // Unregister mailbox from RAM (DashMap)
        app_state_clone.ws_registry.unregister_driver(&user_id);
    }

    send_task.abort();
}
