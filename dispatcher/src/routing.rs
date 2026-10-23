//! Shared, bounded OSRM transport for fare quotes and driver-to-pickup routing.
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub const DEFAULT_BASE_URL: &str = "https://app.freebus.cloud/routing";
const TIMEOUT: Duration = Duration::from_millis(3500);
const CACHE_TTL: Duration = Duration::from_secs(60);
const COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct RoadRoute {
    pub distance: f64,
    pub duration: f64,
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingError {
    NoRoute,
    InvalidRequest,
    Unavailable,
    Configuration,
}
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoRoute => "No road route available for these locations",
            Self::InvalidRequest => "Invalid routing request",
            Self::Unavailable => "Routing temporarily unavailable",
            Self::Configuration => "Invalid routing service configuration",
        })
    }
}
impl std::error::Error for RoutingError {}

#[derive(Default)]
struct State {
    cache: HashMap<String, (Instant, RoadRoute)>,
    outages: HashMap<String, Instant>,
    next_request: Option<Instant>,
}

/// One pooled client and shared cache/rate gate per backend process.
pub struct Router {
    client: reqwest::Client,
    state: Mutex<State>,
}
impl Default for Router {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("routing HTTP client"),
            state: Mutex::new(State::default()),
        }
    }
}

/// Resolves configured primary without silently selecting a public demo provider.
pub fn primary_url() -> String {
    std::env::var("OSRM_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into())
}

/// Uses the same process-wide provider state for every fare and dispatch request.
pub async fn route(
    primary: &str,
    start: [f64; 2],
    end: [f64; 2],
) -> Result<RoadRoute, RoutingError> {
    static ROUTER: OnceLock<Router> = OnceLock::new();
    let backup = std::env::var("ROUTING_FALLBACK_API_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty());
    ROUTER
        .get_or_init(Router::default)
        .route(primary, backup.as_deref(), start, end)
        .await
}

/// Validates service URLs, including both known non-commercial demo hosts.
fn validate_base(base: &str) -> Result<(), RoutingError> {
    let url = reqwest::Url::parse(base).map_err(|_| RoutingError::Configuration)?;
    let host = url
        .host_str()
        .ok_or(RoutingError::Configuration)?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if !matches!(url.scheme(), "http" | "https")
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || ["router.project-osrm.org", "routing.openstreetmap.de"]
            .iter()
            .any(|demo| host == *demo || host.ends_with(&format!(".{demo}")))
    {
        return Err(RoutingError::Configuration);
    }
    Ok(())
}

impl Router {
    /// Retries on a configured backup only for transient/invalid upstream responses.
    pub async fn route(
        &self,
        primary: &str,
        backup: Option<&str>,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Result<RoadRoute, RoutingError> {
        validate_base(primary)?;
        if let Some(base) = backup {
            validate_base(base)?;
        }
        for point in [start, end] {
            if !point[0].is_finite()
                || !point[1].is_finite()
                || point[0].abs() > 180.0
                || point[1].abs() > 90.0
            {
                return Err(RoutingError::InvalidRequest);
            }
        }
        match self.fetch(primary, start, end).await {
            Err(RoutingError::Unavailable)
                if backup
                    .is_some_and(|b| b.trim_end_matches('/') != primary.trim_end_matches('/')) =>
            {
                self.fetch(backup.unwrap(), start, end).await
            }
            result => result,
        }
    }

    /// Coalesces identical misses, bounds queue waits, caches successes and opens a cooldown on outages.
    async fn fetch(
        &self,
        base: &str,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Result<RoadRoute, RoutingError> {
        let url = format!(
            "{}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?overview=full&geometries=geojson&steps=false&radiuses=1000;1000",
            base.trim_end_matches('/'),
            start[0],
            start[1],
            end[0],
            end[1]
        );
        let mut state = tokio::time::timeout(TIMEOUT, self.state.lock())
            .await
            .map_err(|_| RoutingError::Unavailable)?;
        let now = Instant::now();
        state
            .cache
            .retain(|_, (saved, _)| now.duration_since(*saved) < CACHE_TTL);
        if let Some((_, cached)) = state.cache.get(&url) {
            return Ok(cached.clone());
        }
        if state.outages.get(base).is_some_and(|until| *until > now) {
            return Err(RoutingError::Unavailable);
        }
        if let Some(next) = state.next_request {
            tokio::time::sleep(next.saturating_duration_since(now)).await;
        }
        // At most five upstream requests per second, shared by quotes and dispatch in this process.
        state.next_request = Some(Instant::now() + Duration::from_millis(200));
        let result = self.request(&url).await;
        match &result {
            Ok(route) => {
                state.outages.remove(base);
                if state.cache.len() >= 512 {
                    if let Some(oldest) = state
                        .cache
                        .iter()
                        .min_by_key(|(_, (at, _))| *at)
                        .map(|(key, _)| key.clone())
                    {
                        state.cache.remove(&oldest);
                    }
                }
                state.cache.insert(url, (Instant::now(), route.clone()));
            }
            Err(RoutingError::Unavailable) => {
                state.outages.insert(base.into(), Instant::now() + COOLDOWN);
            }
            _ => {}
        }
        result
    }

    /// Distinguishes real coverage failures from outages and validates every route field.
    async fn request(&self, url: &str) -> Result<RoadRoute, RoutingError> {
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| RoutingError::Unavailable)?;
        let status = response.status();
        if status.as_u16() == 429 || status.is_server_error() {
            return Err(RoutingError::Unavailable);
        }
        // Bound decoded response memory even when an upstream omits Content-Length.
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| RoutingError::Unavailable)?
        {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err(RoutingError::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        let data: Response =
            serde_json::from_slice(&bytes).map_err(|_| RoutingError::Unavailable)?;
        if matches!(data.code.as_str(), "NoRoute" | "NoSegment") {
            return Err(RoutingError::NoRoute);
        }
        if status.is_client_error() {
            return Err(RoutingError::InvalidRequest);
        }
        if !status.is_success() || data.code != "Ok" {
            return Err(RoutingError::Unavailable);
        }
        let route = data
            .routes
            .and_then(|r| r.into_iter().next())
            .ok_or(RoutingError::Unavailable)?;
        let geometry = route.geometry.ok_or(RoutingError::Unavailable)?;
        if !route.distance.is_finite()
            || route.distance <= 0.0
            || route.distance > i64::MAX as f64
            || !route.duration.is_finite()
            || route.duration <= 0.0
            || route.duration > i64::MAX as f64
            || geometry.kind != "LineString"
            || geometry.coordinates.len() < 2
            || geometry.coordinates.iter().any(|p| {
                !p[0].is_finite() || !p[1].is_finite() || p[0].abs() > 180.0 || p[1].abs() > 90.0
            })
        {
            return Err(RoutingError::Unavailable);
        }
        Ok(RoadRoute {
            distance: route.distance,
            duration: route.duration,
            coordinates: geometry.coordinates,
        })
    }
}

#[derive(Deserialize)]
struct Response {
    code: String,
    routes: Option<Vec<ResponseRoute>>,
}
#[derive(Deserialize)]
struct ResponseRoute {
    distance: f64,
    duration: f64,
    geometry: Option<Geometry>,
}
#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    kind: String,
    coordinates: Vec<[f64; 2]>,
}
