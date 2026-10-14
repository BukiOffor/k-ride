pub mod types;
pub use types::{DriverState, RankedDriver, RideDispatcher, RouteEstimate};
use types::{OsrmRouteResponse, OsrmTableResponse};

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
            .ignore();

        let _: () = pipe.query_async(&mut self.redis).await?;

        Ok(())
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

    async fn try_osrm_table(
        &self,
        osrm_url: &str,
        pickup_lat: f64,
        pickup_lng: f64,
        candidates: &[DriverState],
    ) -> Result<OsrmTableResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut coords = Vec::with_capacity(candidates.len() + 1);
        coords.push(format!("{:.6},{:.6}", pickup_lng, pickup_lat));
        for d in candidates {
            coords.push(format!("{:.6},{:.6}", d.lng, d.lat));
        }

        let url = format!(
            "{}/table/v1/driving/{}?sources=0&annotations=distance,duration",
            osrm_url.trim_end_matches('/'),
            coords.join(";")
        );

        let resp = self.http_client.get(&url).send().await?.error_for_status()?;
        let data = resp.json::<OsrmTableResponse>().await?;
        if data.code != "Ok" {
            return Err(format!("OSRM returned code: {}", data.code).into());
        }
        Ok(data)
    }

    /// Finds drivers within `radius_m`, ranking by OSRM driving ETA and road distance.
    ///
    /// If `osrm_url` is provided, it queries the OSRM Table API in a single batch.
    /// If OSRM is unavailable, times out, or fails, it automatically falls back
    /// to Haversine straight-line distance.
    pub async fn query_within_radius_routed(
        &mut self,
        pickup_lat: f64,
        pickup_lng: f64,
        radius_m: f64,
        osrm_url: Option<&str>,
    ) -> Result<Vec<RankedDriver>, Box<dyn std::error::Error>> {
        let pickup = LatLng::new(pickup_lat, pickup_lng)?;
        let spacing = VENUE_RESOLUTION.edge_length_m() * 3f64.sqrt();
        let k = (radius_m / spacing).ceil() as u32 + 1; // +1 so the edges aren't missed

        let candidates = self.query_candidates(pickup_lat, pickup_lng, k).await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Try OSRM routing if URL is provided
        if let Some(url) = osrm_url {
            match self.try_osrm_table(url, pickup_lat, pickup_lng, &candidates).await {
                Ok(osrm_resp) => {
                    if let (Some(distances), Some(durations)) = (osrm_resp.distances, osrm_resp.durations) {
                        if !distances.is_empty() && !durations.is_empty() {
                            let dist_row = &distances[0];
                            let dur_row = &durations[0];
                            let mut results = Vec::new();

                            for (i, driver) in candidates.into_iter().enumerate() {
                                let osrm_dist = dist_row.get(i + 1).and_then(|v| *v);
                                let osrm_dur = dur_row.get(i + 1).and_then(|v| *v);

                                // Use OSRM road distance if available, otherwise Haversine
                                let dist = match osrm_dist {
                                    Some(d) => d,
                                    None => match LatLng::new(driver.lat, driver.lng) {
                                        Ok(c) => c.distance_m(pickup),
                                        Err(_) => continue,
                                    },
                                };

                                if dist <= radius_m {
                                    results.push(RankedDriver {
                                        driver,
                                        distance_m: dist,
                                        duration_secs: osrm_dur,
                                        is_osrm_routed: osrm_dur.is_some(),
                                    });
                                }
                            }

                            // Sort by driving ETA (duration) if available, otherwise distance
                            results.sort_by(|a, b| match (a.duration_secs, b.duration_secs) {
                                (Some(d1), Some(d2)) => d1.total_cmp(&d2),
                                _ => a.distance_m.total_cmp(&b.distance_m),
                            });

                            return Ok(results);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Warning: OSRM query failed ({}), falling back to Haversine.", err);
                }
            }
        }

        // 2. Fallback: Haversine straight-line distance
        let mut results: Vec<RankedDriver> = candidates
            .into_iter()
            .filter_map(|d| {
                let dist = LatLng::new(d.lat, d.lng).ok()?.distance_m(pickup);
                (dist <= radius_m).then_some(RankedDriver {
                    driver: d,
                    distance_m: dist,
                    duration_secs: None,
                    is_osrm_routed: false,
                })
            })
            .collect();

        results.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
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

    /// Calculates driving distance and ETA between a pickup and dropoff location.
    ///
    /// If `osrm_url` is provided, it queries the OSRM Route API for exact turn-by-turn
    /// road distance (meters) and duration (seconds).
    /// If OSRM is unreachable or fails, it automatically falls back to Haversine straight-line
    /// distance and estimates travel duration based on an average urban driving speed.
    pub async fn estimate_trip(
        &self,
        pickup_lat: f64,
        pickup_lng: f64,
        dropoff_lat: f64,
        dropoff_lng: f64,
        osrm_url: Option<&str>,
    ) -> Result<RouteEstimate, Box<dyn std::error::Error>> {
        if let Some(url) = osrm_url {
            let req_url = format!(
                "{}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?overview=false",
                url.trim_end_matches('/'),
                pickup_lng,
                pickup_lat,
                dropoff_lng,
                dropoff_lat
            );

            match self.http_client.get(&req_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(data) = resp.json::<OsrmRouteResponse>().await {
                        if data.code == "Ok" {
                            if let Some(routes) = data.routes {
                                if let Some(route) = routes.first() {
                                    return Ok(RouteEstimate {
                                        distance_m: route.distance,
                                        duration_secs: route.duration,
                                        is_osrm_routed: true,
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(resp) => {
                    eprintln!(
                        "Warning: OSRM route request returned status {}, falling back to Haversine.",
                        resp.status()
                    );
                }
                Err(err) => {
                    eprintln!(
                        "Warning: OSRM route request failed ({}), falling back to Haversine.",
                        err
                    );
                }
            }
        }

        // Fallback: Haversine distance with urban speed estimation
        let pickup = LatLng::new(pickup_lat, pickup_lng)?;
        let dropoff = LatLng::new(dropoff_lat, dropoff_lng)?;
        let straight_line_m = pickup.distance_m(dropoff);

        // In urban road networks, actual route distance is typically ~35% longer than straight-line
        let estimated_road_dist_m = straight_line_m * 1.35;
        // Average urban driving speed: ~30 km/h (8.33 m/s)
        let avg_speed_mps = 30.0 * 1000.0 / 3600.0;
        let duration_secs = estimated_road_dist_m / avg_speed_mps;

        Ok(RouteEstimate {
            distance_m: straight_line_m,
            duration_secs,
            is_osrm_routed: false,
        })
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