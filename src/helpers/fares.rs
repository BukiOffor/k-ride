//! Dynamic Fare Estimation and Routing Engine
//! Supports OSRM with fallback to Google Directions or Haversine mock calculation.
//! Implements waiting-time calculation (free 5-min period + per-additional-minute rate).

use crate::vrp::dtos::rides::{
    FareLine, FareQuote, FareRange, LatLng, Money, RouteSource, WaitingPolicy,
};
use crate::vrp::models::rides::RideOption;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub points: Vec<LatLng>,
    pub distance_meters: i64,
    pub duration_seconds: i64,
    pub source: RouteSource,
}

/// Fetch route from OSRM, Google Directions, or fallback mock
pub async fn compute_route(
    client: &reqwest::Client,
    pickup: &LatLng,
    destination: &LatLng,
) -> RouteResult {
    // 1. Try OSRM if configured via OSRM_URL or default
    let osrm_base = std::env::var("OSRM_URL").ok();
    if let Some(ref url) = osrm_base {
        if let Ok(res) = fetch_osrm_route(client, url, pickup, destination).await {
            return res;
        }
    }

    // 2. Try Google Directions if GOOGLE_MAPS_API_KEY is configured
    if let Ok(api_key) = std::env::var("GOOGLE_MAPS_API_KEY") {
        if let Ok(res) = fetch_google_route(client, &api_key, pickup, destination).await {
            return res;
        }
    }

    // 3. Fallback: Mock / Haversine road-approximated calculation
    fallback_mock_route(pickup, destination)
}

#[derive(Deserialize)]
struct OsrmRouteResp {
    code: String,
    routes: Option<Vec<OsrmRoute>>,
}

#[derive(Deserialize)]
struct OsrmRoute {
    distance: f64,
    duration: f64,
    geometry: Option<serde_json::Value>,
}

async fn fetch_osrm_route(
    client: &reqwest::Client,
    base_url: &str,
    pickup: &LatLng,
    destination: &LatLng,
) -> Result<RouteResult, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "{}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?overview=full&geometries=geojson",
        base_url.trim_end_matches('/'),
        pickup.lng,
        pickup.lat,
        destination.lng,
        destination.lat
    );

    let resp: OsrmRouteResp = client.get(&url).send().await?.json().await?;
    if resp.code != "Ok" {
        return Err("OSRM route code not Ok".into());
    }

    let route = resp
        .routes
        .and_then(|r| r.into_iter().next())
        .ok_or("No routes in OSRM response")?;

    let mut points = Vec::new();
    if let Some(geom) = route.geometry {
        // GeoJSON coords: [[lng, lat], ...]
        if let Some(coords) = geom.get("coordinates").and_then(|c| c.as_array()) {
            for coord in coords {
                if let Some(pair) = coord.as_array() {
                    if pair.len() >= 2 {
                        let lng = pair[0].as_f64().unwrap_or_default();
                        let lat = pair[1].as_f64().unwrap_or_default();
                        points.push(LatLng { lat, lng });
                    }
                }
            }
        }
    }

    if points.is_empty() {
        points.push(*pickup);
        points.push(*destination);
    }

    Ok(RouteResult {
        points,
        distance_meters: route.distance.round() as i64,
        duration_seconds: route.duration.round() as i64,
        source: RouteSource::Road,
    })
}

#[derive(Deserialize)]
struct GoogleDirectionsResp {
    status: String,
    routes: Option<Vec<GoogleRoute>>,
}

#[derive(Deserialize)]
struct GoogleRoute {
    legs: Option<Vec<GoogleLeg>>,
}

#[derive(Deserialize)]
struct GoogleLeg {
    distance: Option<GoogleValue>,
    duration: Option<GoogleValue>,
}

#[derive(Deserialize)]
struct GoogleValue {
    value: i64,
}

async fn fetch_google_route(
    client: &reqwest::Client,
    api_key: &str,
    pickup: &LatLng,
    destination: &LatLng,
) -> Result<RouteResult, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://maps.googleapis.com/maps/api/directions/json?origin={:.6},{:.6}&destination={:.6},{:.6}&key={}",
        pickup.lat, pickup.lng, destination.lat, destination.lng, api_key
    );

    let resp: GoogleDirectionsResp = client.get(&url).send().await?.json().await?;
    if resp.status != "OK" {
        return Err(format!("Google Directions status: {}", resp.status).into());
    }

    let leg = resp
        .routes
        .and_then(|r| r.into_iter().next())
        .and_then(|r| r.legs)
        .and_then(|l| l.into_iter().next())
        .ok_or("No leg found in Google response")?;

    let dist = leg.distance.map(|d| d.value).unwrap_or(0);
    let dur = leg.duration.map(|d| d.value).unwrap_or(0);

    Ok(RouteResult {
        points: vec![*pickup, *destination],
        distance_meters: dist,
        duration_seconds: dur,
        source: RouteSource::Road,
    })
}

/// Fallback Haversine straight-line multiplied by road tortuosity factor (~1.35)
pub fn fallback_mock_route(pickup: &LatLng, destination: &LatLng) -> RouteResult {
    let d_lat = (destination.lat - pickup.lat).to_radians();
    let d_lng = (destination.lng - pickup.lng).to_radians();
    let lat1 = pickup.lat.to_radians();
    let lat2 = destination.lat.to_radians();

    let a = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    let earth_radius_m = 6_371_000.0;
    let straight_line_m = earth_radius_m * c;

    // Road factor ~1.35
    let distance_meters = (straight_line_m * 1.35).round() as i64;
    // Avg city speed ~30 km/h = 8.33 m/s
    let duration_seconds = (distance_meters as f64 / 8.33).round() as i64;

    RouteResult {
        points: vec![*pickup, *destination],
        distance_meters,
        duration_seconds,
        source: RouteSource::Approximate,
    }
}

/// Calculate dynamic fare quote for a specific RideOption based on route distance and duration
pub fn calculate_fare_quote(
    option: &RideOption,
    distance_meters: i64,
    duration_seconds: i64,
) -> FareQuote {
    let km = distance_meters as f64 / 1000.0;
    let minutes = duration_seconds as f64 / 60.0;

    let distance_fare = (km * option.per_km_minor as f64).round() as i64;
    let time_fare = (minutes * option.per_minute_minor as f64).round() as i64;

    let subtotal = option.base_fare_minor + distance_fare + time_fare + option.booking_fee_minor;
    let mut total = subtotal.max(option.minimum_fare_minor);

    // Apply rounding if configured (e.g., 5000 minor units = 50 NGN)
    if option.rounding_minor > 0 {
        let remainder = total % option.rounding_minor;
        if remainder > 0 {
            total += option.rounding_minor - remainder;
        }
    }

    let low = (total as f64 * 0.95).round() as i64;
    let high = (total as f64 * 1.10).round() as i64;

    let breakdown = vec![
        FareLine {
            code: "base".into(),
            label: "Base Fare".into(),
            amount: Money::new(option.base_fare_minor, &option.currency),
        },
        FareLine {
            code: "distance".into(),
            label: format!("{:.1} km", km),
            amount: Money::new(distance_fare, &option.currency),
        },
        FareLine {
            code: "time".into(),
            label: format!("{:.0} mins", minutes),
            amount: Money::new(time_fare, &option.currency),
        },
    ];

    FareQuote {
        total: Money::new(total, &option.currency),
        range: FareRange {
            low: Money::new(low, &option.currency),
            high: Money::new(high, &option.currency),
        },
        breakdown,
        is_estimate: true,
        waiting: WaitingPolicy {
            free_minutes: option.free_waiting_minutes,
            per_minute: Money::new(option.waiting_per_minute_minor, &option.currency),
        },
    }
}

/// Calculate waiting time charge:
/// Free for the first 5 minutes (or option.free_waiting_minutes).
/// Each additional started minute after the free window incurs option.waiting_per_minute_minor.
pub fn calculate_waiting_charge(
    option: &RideOption,
    arrived_at: chrono::NaiveDateTime,
    started_at: chrono::NaiveDateTime,
) -> (i64, i64, i64) {
    let waited_seconds = (started_at - arrived_at).num_seconds().max(0);
    let free_seconds = (option.free_waiting_minutes as i64) * 60;

    if waited_seconds <= free_seconds {
        return (waited_seconds, 0, 0);
    }

    let billable_seconds = waited_seconds - free_seconds;
    // Charge per additional started minute (ceil)
    let billable_minutes = (billable_seconds + 59) / 60;
    let charge_minor = billable_minutes * option.waiting_per_minute_minor;

    (waited_seconds, billable_minutes, charge_minor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_waiting_charge_within_free_window() {
        let opt = RideOption {
            id: "go".into(),
            name: "Go".into(),
            description: "".into(),
            vehicle_class: "sedan".into(),
            eligible_vehicle_classes: vec![],
            capacity: 4,
            shared: false,
            currency: "NGN".into(),
            base_fare_minor: 50000,
            per_km_minor: 15000,
            per_minute_minor: 2000,
            minimum_fare_minor: 100000,
            booking_fee_minor: 0,
            free_waiting_minutes: 5,
            waiting_per_minute_minor: 3000,
            rounding_minor: 5000,
            is_active: true,
            sort_order: 1,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        let arrived = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        // 4 minutes later
        let started = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 4, 30)
            .unwrap();

        let (waited, billable_mins, charge) = calculate_waiting_charge(&opt, arrived, started);
        assert_eq!(waited, 270);
        assert_eq!(billable_mins, 0);
        assert_eq!(charge, 0);

        // 8 minutes later (3 additional minutes)
        let started_late = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 8, 0)
            .unwrap();
        let (waited_late, billable_late, charge_late) =
            calculate_waiting_charge(&opt, arrived, started_late);
        assert_eq!(waited_late, 480);
        assert_eq!(billable_late, 3);
        assert_eq!(charge_late, 3 * 3000);
    }
}
