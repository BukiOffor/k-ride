pub mod types;
pub mod routing;
pub use types::{DriverState, RankedDriver, RideDispatcher, RouteEstimate};

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use h3o::{CellIndex, LatLng, Resolution};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

// Resolution 6: ~3.7 km edges, ~33 km² per cell. A 25 km (city-wide Abuja) search
// needs only ~91 cells; exact distance filtering in `query_within_radius` keeps results precise.
const VENUE_RESOLUTION: Resolution = Resolution::Six;

/// A driver is considered offline if it hasn't pinged for this many seconds (15-second dead-man's switch).
const DRIVER_TTL_SECS: u64 = 15;

/// Global sorted set of every live driver, scored by last-seen time (used for fleet-wide views).
const LIVE_DRIVERS_KEY: &str = "drivers:live";


impl RideDispatcher {
    pub fn new(redis: ConnectionManager) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(4))
            .user_agent("k-rides/0.1.0 (contact: ebuka2264@yahoo.com)")
            .build()
            .unwrap_or_default();
        Self { redis, http_client }
    }

    /// Converts raw GPS coordinates into an H3 CellIndex.
    pub fn coordinate_to_cell(&self, lat: f64, lng: f64) -> Result<CellIndex, Box<dyn std::error::Error>> {
        let coord = LatLng::new(lat, lng)?;
        Ok(coord.to_cell(VENUE_RESOLUTION))
    }

    /// Removes a driver immediately from the spatial index and cell tracking when going offline.
    pub async fn remove_driver(&mut self, driver_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let driver_key = format!("driver:state:{}", driver_id);
        let driver_cell_key = format!("driver:cell:{}", driver_id);

        let previous_cell: Option<String> = self.redis.get(&driver_cell_key).await?;

        let mut pipe = redis::pipe();
        pipe.atomic();

        if let Some(cell) = previous_cell {
            pipe.zrem(cell_active_key(&cell), driver_id).ignore();
        }

        pipe.zrem(LIVE_DRIVERS_KEY, driver_id).ignore();
        pipe.del(&driver_key).ignore();
        pipe.del(&driver_cell_key).ignore();

        let _: () = pipe.query_async(&mut self.redis).await?;
        Ok(())
    }

    /// Ingests real-time driver GPS ping and updates cell indices.
    ///
    /// If the driver has moved to a different cell since its last ping, it is
    /// removed from the old cell so it can't be found in two places at once.
    pub async fn ingest_driver_location(
        &mut self,
        driver: &DriverState,
    ) -> Result<(), Box<dyn std::error::Error>> {

        let cell = self.coordinate_to_cell(driver.lat, driver.lng)?;
        let cell_hex = cell.to_string();

        let driver_key = format!("driver:state:{}", driver.driver_id);
        let driver_cell_key = format!("driver:cell:{}", driver.driver_id);
        let active_key = cell_active_key(&cell_hex);

        let payload = bincode::serialize(driver)?;

        // Which cell was this driver in last time?
        let previous_cell: Option<String> = self.redis.get(&driver_cell_key).await?;

        // Apply all writes together in one atomic transaction (MULTI/EXEC).
        let mut pipe = redis::pipe();
        pipe.atomic();

        // 1. Moved to a new cell? Remove it from the old cell's set.
        if let Some(prev) = previous_cell.filter(|p| *p != cell_hex) {
            pipe.zrem(cell_active_key(&prev), &driver.driver_id).ignore();
        }

        pipe
            // 2. Store driver state as compact binary payload (Bincode)
            .set_ex(&driver_key, payload, DRIVER_TTL_SECS)
            .ignore()
            // 3. Remember the driver's current cell for the next ping
            .set_ex(&driver_cell_key, &cell_hex, DRIVER_TTL_SECS)
            .ignore()
            // 4. Add/refresh driver in the cell, scored by last-seen time
            .zadd(&active_key, &driver.driver_id, now_secs())
            .ignore()
            // 5. Let empty cells disappear eventually (garbage collection only)
            .expire(&active_key, DRIVER_TTL_SECS as i64)
            .ignore()
            // 6. Track the driver in the global live set (fleet-wide map view)
            .zadd(LIVE_DRIVERS_KEY, &driver.driver_id, now_secs())
            .ignore();

        let _: () = pipe.query_async(&mut self.redis).await?;

        Ok(())
    }

    /// Returns every live driver (pinged within the TTL window) with their last-seen
    /// Unix timestamp in seconds. Stale entries are pruned on read.
    pub async fn list_live_drivers(
        &mut self,
    ) -> Result<Vec<(DriverState, f64)>, Box<dyn std::error::Error>> {
        let cutoff = now_secs() - DRIVER_TTL_SECS as f64;

        let (entries,): (Vec<(String, f64)>,) = redis::pipe()
            .zrembyscore(LIVE_DRIVERS_KEY, "-inf", cutoff)
            .ignore()
            .zrangebyscore_withscores(LIVE_DRIVERS_KEY, cutoff, "+inf")
            .query_async(&mut self.redis)
            .await?;

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let keys: Vec<String> = entries
            .iter()
            .map(|(id, _)| format!("driver:state:{}", id))
            .collect();
        let raw_states: Vec<Option<Vec<u8>>> = self.redis.mget(&keys).await?;

        Ok(entries
            .into_iter()
            .zip(raw_states)
            .filter_map(|((_, last_seen), raw)| {
                let state = bincode::deserialize::<DriverState>(&raw?).ok()?;
                Some((state, last_seen))
            })
            .collect())
    }

    /// Finds candidate drivers in the pickup cell and k concentric neighbor rings.
    ///
    /// Uses a single pipelined batch query across all neighbor cells to eliminate
    /// sequential network round-trips, followed by an MGET batch to retrieve
    /// compact binary (Bincode) driver payloads.
    pub async fn query_candidates(
        &mut self,
        pickup_lat: f64,
        pickup_lng: f64,
        k_rings: u32,
    ) -> Result<Vec<DriverState>, Box<dyn std::error::Error>> {
        let origin_cell = self.coordinate_to_cell(pickup_lat, pickup_lng)?;

        // Generates the origin cell (k=0) + surrounding rings
        let neighbor_cells: Vec<CellIndex> = origin_cell
            .grid_disk_fast(k_rings)
            .flatten()
            .collect();

        if neighbor_cells.is_empty() {
            return Ok(Vec::new());
        }

        // Anyone not seen since this time is treated as offline
        let cutoff = now_secs() - DRIVER_TTL_SECS as f64;

        // BATCH 1: Pipeline all neighbor cells in a single Redis roundtrip
        let mut cell_pipe = redis::pipe();
        for cell in &neighbor_cells {
            let active_key = cell_active_key(&cell.to_string());
            // Prune expired drivers in the cell
            cell_pipe.zrembyscore(&active_key, "-inf", cutoff).ignore();
            // Fetch active drivers within the TTL window
            cell_pipe.zrangebyscore(&active_key, cutoff, "+inf");
        }

        let cell_driver_ids: Vec<Vec<String>> = cell_pipe.query_async(&mut self.redis).await?;

        // Deduplicate driver IDs across adjacent cells
        let mut seen = HashSet::new();
        let mut unique_driver_ids = Vec::new();
        for ids in cell_driver_ids {
            for id in ids {
                if seen.insert(id.clone()) {
                    unique_driver_ids.push(id);
                }
            }
        }

        if unique_driver_ids.is_empty() {
            return Ok(Vec::new());
        }

        // BATCH 2: Fetch all unique driver states in a single roundtrip via MGET
        let driver_keys: Vec<String> = unique_driver_ids
            .iter()
            .map(|id| format!("driver:state:{}", id))
            .collect();

        let raw_states: Vec<Option<Vec<u8>>> = self.redis.mget(&driver_keys).await?;

        let mut candidate_drivers = Vec::with_capacity(raw_states.len());
        for raw in raw_states {
            if let Some(bytes) = raw {
                if let Ok(state) = bincode::deserialize::<DriverState>(&bytes) {
                    if state.available_seats > 0 {
                        candidate_drivers.push(state);
                    }
                }
            }
        }

        Ok(candidate_drivers)
    }

    /// Uses spatial distance only to shortlist; routed ranking never invents road ETAs.
    /// The exposed gateway has no Table API. Route at most five nearby candidates
    /// from driver to pickup, with a five-second budget for the whole shortlist.
    pub async fn query_within_radius_routed(
        &mut self,
        pickup_lat: f64,
        pickup_lng: f64,
        radius_m: f64,
        osrm_url: Option<&str>,
    ) -> Result<Vec<RankedDriver>, Box<dyn std::error::Error>> {
        let pickup = LatLng::new(pickup_lat, pickup_lng)?;
        let spacing = VENUE_RESOLUTION.edge_length_m() * 3f64.sqrt();
        let k = (radius_m / spacing).ceil() as u32 + 1;
        let candidates = self.query_candidates(pickup_lat, pickup_lng, k).await?;
        let mut shortlist: Vec<_> = candidates.into_iter().filter_map(|driver| {
            let distance = LatLng::new(driver.lat, driver.lng).ok()?.distance_m(pickup);
            (distance <= radius_m).then_some(RankedDriver {
                driver, distance_m: distance, duration_secs: None, is_osrm_routed: false,
            })
        }).collect();
        shortlist.sort_by(|a,b| a.distance_m.total_cmp(&b.distance_m));
        let Some(base) = osrm_url else { return Ok(shortlist); };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut results = Vec::new();
        for candidate in shortlist.into_iter().take(5) {
            match tokio::time::timeout_at(deadline, routing::route(base,
                [candidate.driver.lng, candidate.driver.lat], [pickup_lng, pickup_lat])).await {
                Ok(Ok(route)) if route.distance <= radius_m => results.push(RankedDriver {
                    driver: candidate.driver, distance_m: route.distance,
                    duration_secs: Some(route.duration), is_osrm_routed: true,
                }),
                Ok(Err(routing::RoutingError::Unavailable | routing::RoutingError::Configuration)) | Err(_) => break,
                _ => {},
            }
        }
        results.sort_by(|a,b| a.duration_secs.unwrap().total_cmp(&b.duration_secs.unwrap()));
        Ok(results)
    }

    /// Finds drivers within `radius_m` metres of the pickup point using Haversine distance,
    /// sorted nearest first.
    pub async fn query_within_radius(
        &mut self,
        pickup_lat: f64,
        pickup_lng: f64,
        radius_m: f64,
    ) -> Result<Vec<(DriverState, f64)>, Box<dyn std::error::Error>> {
        let ranked = self
            .query_within_radius_routed(pickup_lat, pickup_lng, radius_m, None)
            .await?;
        Ok(ranked.into_iter().map(|r| (r.driver, r.distance_m)).collect())
    }

    /// Returns validated road distance and duration, or an explicit routing error.
    pub async fn estimate_trip(
        &self,
        pickup_lat: f64,
        pickup_lng: f64,
        dropoff_lat: f64,
        dropoff_lng: f64,
        osrm_url: Option<&str>,
    ) -> Result<RouteEstimate, Box<dyn std::error::Error>> {
        let base = osrm_url.map(str::to_owned).unwrap_or_else(routing::primary_url);
        let route = routing::route(&base, [pickup_lng, pickup_lat], [dropoff_lng, dropoff_lat]).await?;
        Ok(RouteEstimate { distance_m: route.distance, duration_secs: route.duration, is_osrm_routed: true })
    }

}

/// Current Unix time in seconds (with fractions).
fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Redis key of the sorted set holding drivers in a cell, scored by last-seen time.
fn cell_active_key(cell: &str) -> String {
    format!("h3:{}:active", cell)
}