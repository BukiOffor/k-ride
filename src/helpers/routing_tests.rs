//! Local HTTP fixtures verify routing policy without calling production providers.
use dispatcher::routing::{Router, RoutingError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const GOOD: &str = r#"{"code":"Ok","routes":[{"distance":7234.5,"duration":650.4,"geometry":{"type":"LineString","coordinates":[[7.4246,9.0721],[7.4728,9.0647]]}}]}"#;
const START: [f64; 2] = [7.4246, 9.0721];
const END: [f64; 2] = [7.4728, 9.0647];

struct Server {
    base: String,
    hits: Arc<AtomicUsize>,
    paths: Arc<std::sync::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
/// Serves a fixed response and records requests so tests can assert cache/failover behavior.
async fn server(status: u16, body: &str) -> Server {
    delayed_server(status, body, std::time::Duration::ZERO).await
}

/// A delayed fixture exercises the real client timeout rather than mocking its result.
async fn delayed_server(status: u16, body: &str, delay: std::time::Duration) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/routing", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
    let count = hits.clone();
    let requests = paths.clone();
    let reply = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            requests
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&bytes[..n]).into_owned());
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    Server {
        base,
        hits,
        paths,
        task,
    }
}

#[tokio::test]
async fn road_route_keeps_units_coordinate_order_and_coalesces_requests() {
    let upstream = server(200, GOOD).await;
    let router = Router::default();
    let (a, b) = tokio::join!(
        router.route(&upstream.base, None, START, END),
        router.route(&upstream.base, None, START, END)
    );
    assert_eq!(a.unwrap().distance, 7234.5);
    let route = b.unwrap();
    assert_eq!(route.duration, 650.4);
    assert_eq!(route.coordinates[0], START);
    assert_eq!(upstream.hits.load(Ordering::SeqCst), 1);
    let paths = upstream.paths.lock().unwrap();
    assert!(
        paths[0].starts_with("GET /routing/route/v1/driving/7.424600,9.072100;7.472800,9.064700?")
    );
    assert!(paths[0].contains("steps=false&radiuses=1000;1000"));
}

#[tokio::test]
async fn outage_uses_backup_and_cooldown() {
    for status in [429, 503] {
        let primary = server(status, "{}").await;
        let backup = server(200, GOOD).await;
        let router = Router::default();
        for _ in 0..2 {
            assert!(
                router
                    .route(&primary.base, Some(&backup.base), START, END)
                    .await
                    .is_ok()
            );
        }
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(backup.hits.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn coverage_failures_never_use_backup() {
    for (status, code) in [(400, "NoSegment"), (200, "NoRoute")] {
        let primary = server(status, &format!(r#"{{"code":"{code}"}}"#)).await;
        let backup = server(200, GOOD).await;
        let result = Router::default()
            .route(&primary.base, Some(&backup.base), START, END)
            .await;
        assert_eq!(result.unwrap_err(), RoutingError::NoRoute);
        assert_eq!(backup.hits.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn malformed_and_zero_routes_cannot_produce_quotes() {
    for body in [
        "not json",
        r#"{"code":"Ok","routes":[]}"#,
        r#"{"code":"Ok","routes":[{"distance":0,"duration":0,"geometry":{"type":"LineString","coordinates":[[7,9],[7,9]]}}]}"#,
        r#"{"code":"Ok","routes":[{"distance":100,"duration":20}]}"#,
    ] {
        let primary = server(200, body).await;
        let router = Router::default();
        assert_eq!(
            router
                .route(&primary.base, None, START, END)
                .await
                .unwrap_err(),
            RoutingError::Unavailable
        );
        let backup = server(200, GOOD).await;
        assert!(
            router
                .route(&primary.base, Some(&backup.base), START, END)
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn invalid_request_and_demo_hosts_do_not_fall_back() {
    let backup = server(200, GOOD).await;
    for base in [
        "https://router.project-osrm.org",
        "https://routing.openstreetmap.de",
        "https://ROUTER.PROJECT-OSRM.ORG./",
        "ftp://example.com",
        "https://example.com?x=y",
    ] {
        assert_eq!(
            Router::default()
                .route(base, Some(&backup.base), START, END)
                .await
                .unwrap_err(),
            RoutingError::Configuration
        );
    }
    let primary = server(400, r#"{"code":"InvalidValue"}"#).await;
    assert_eq!(
        Router::default()
            .route(&primary.base, Some(&backup.base), START, END)
            .await
            .unwrap_err(),
        RoutingError::InvalidRequest
    );
    assert_eq!(backup.hits.load(Ordering::SeqCst), 0);
    assert_eq!(
        Router::default()
            .route(&primary.base, None, [190.0, 9.0], END)
            .await
            .unwrap_err(),
        RoutingError::InvalidRequest
    );
}

#[tokio::test]
async fn timeout_and_connection_failure_use_backup() {
    let slow = delayed_server(200, GOOD, std::time::Duration::from_secs(5)).await;
    let backup = server(200, GOOD).await;
    let started = std::time::Instant::now();
    assert!(
        Router::default()
            .route(&slow.base, Some(&backup.base), START, END)
            .await
            .is_ok()
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(slow.hits.load(Ordering::SeqCst), 1);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(
        Router::default()
            .route(&unavailable, Some(&backup.base), START, END)
            .await
            .is_ok()
    );
}
