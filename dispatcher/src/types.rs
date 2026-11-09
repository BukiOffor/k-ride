
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct RideDispatcher {
    pub redis: ConnectionManager,
    pub http_client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverState {
    pub driver_id: String,
    pub lat: f64,
    pub lng: f64,
    pub available_seats: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedDriver {
    pub driver: DriverState,
    pub distance_m: f64,
    pub duration_secs: Option<f64>,
    pub is_osrm_routed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEstimate {
    pub distance_m: f64,
    pub duration_secs: f64,
    pub is_osrm_routed: bool,
}

impl RouteEstimate {
    /// Trip duration in minutes
    pub fn duration_mins(&self) -> f64 {
        self.duration_secs / 60.0
    }
}
