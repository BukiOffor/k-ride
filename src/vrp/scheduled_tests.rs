//! Opt-in integration coverage uses only disposable PostgreSQL/Redis and a local fake email API.
use super::{
    dtos::{rides::*, scheduled::*},
    scheduled_worker,
    services::scheduled::*,
};
use crate::{AppState, ModuleError, auth::jwt::Claims, users::RoleType};
use chrono::{Duration, Utc};
use diesel_async::AsyncConnection;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU16, AtomicUsize, Ordering},
};
use uuid::Uuid;

/// Check the booking window and bounded retry policy without external services.
#[test]
fn booking_window_and_mail_backoff() {
    let now = Utc::now();
    assert!(validate_pickup(now + Duration::minutes(59), now).is_err());
    assert!(validate_pickup(now + Duration::hours(2), now).is_ok());
    assert!(validate_pickup(now + Duration::days(31), now).is_err());
    assert_eq!(crate::mailer::worker::retry_seconds(1), 30);
    assert_eq!(crate::mailer::worker::retry_seconds(2), 60);
    assert_eq!(crate::mailer::worker::retry_seconds(20), 900);
}
/// Reclaim idle future-booking channels without racing an in-progress subscriber.
#[test]
fn idle_booking_channels_are_reclaimed() {
    let registry = super::realtime::WsRegistry::new();
    let name = "scheduled:test";
    let sender = registry.admin_channel(name);
    registry.cleanup_stale_connections(std::time::Duration::from_secs(45));
    assert!(registry.admin_channels.contains_key(name));
    let receiver = sender.subscribe();
    drop(sender);
    registry.cleanup_stale_connections(std::time::Duration::from_secs(45));
    assert!(registry.admin_channels.contains_key(name));
    drop(receiver);
    registry.cleanup_stale_connections(std::time::Duration::from_secs(45));
    assert!(!registry.admin_channels.contains_key(name));
}
/// Construct test claims without generating tokens or loading production credentials.
fn claims(user_id: Uuid, role: RoleType) -> Claims {
    Claims {
        user_id,
        role,
        exp: usize::MAX,
    }
}
/// Create an isolated identity and, for drivers, an approved private-ride vehicle/profile.
async fn user(s: &AppState, driver: bool) -> Uuid {
    let id = Uuid::now_v7();
    let mut db = connection(s).await.unwrap();
    query(&mut db,"INSERT INTO users(id,first_name,last_name,email,username,phone,address,country,role,city,password_hash) VALUES($2,'Scheduled','Test',$2::text || '@example.invalid',$2::text,$2::text,'test','NG',$1->>'role','Test','unused') RETURNING '{}'::jsonb AS data",json!({"role":if driver{"Driver"}else{"User"}}),id).await.unwrap();
    if driver {
        let car = Uuid::now_v7();
        query(&mut db,"INSERT INTO cars(id,license_plate,capacity,car_type,car_tier,car_size,color,brand,model,year,is_verified) VALUES($2,$2::text,4,'Saloon','Comfort','Standard','blue','Test','Car',2026,true) RETURNING '{}'::jsonb AS data",json!({}),car).await.unwrap();
        query(&mut db,"INSERT INTO driver_profiles(id,user_id,car_id,nin_slip,profile_photo,is_koinonia_worker,licence_number,licence_class,licence_expiry_date,licence_photo,track,status) VALUES($2,$2,($1->>'car')::uuid,'data','data',true,'test','B',CURRENT_DATE+365,'data','Professional','APPROVED') RETURNING '{}'::jsonb AS data",json!({"car":car}),id).await.unwrap();
        query(
            &mut db,
            "UPDATE users SET car=($1->>'car')::uuid WHERE id=$2 RETURNING '{}'::jsonb AS data",
            json!({"car":car}),
            id,
        )
        .await
        .unwrap();
    }
    id
}
/// Store a valid quote built by the real fare calculator so activation exercises the existing projections.
async fn estimate(s: &AppState, passenger: Uuid) -> Uuid {
    let mut db = connection(s).await.unwrap();
    let option=query(&mut db,"SELECT to_jsonb(o) AS data FROM ride_options o WHERE id='go' AND $1 IS NOT NULL AND $2 IS NOT NULL",json!({}),passenger).await.unwrap();
    let option: super::models::rides::RideOption = serde_json::from_value(option).unwrap();
    let fare = crate::helpers::fares::calculate_fare_quote(&option, 10000, 1800);
    let id = Uuid::now_v7();
    let place = json!({"name":"Test pickup","point":{"lat":9.0,"lng":7.0}});
    let destination = json!({"name":"Test destination","point":{"lat":9.01,"lng":7.01}});
    let route = json!({"points":[],"distanceMeters":10000,"durationSeconds":1800,"source":"road"});
    query(&mut db,"INSERT INTO fare_estimates(id,user_id,pickup,destination,route,options,expires_at,created_at) VALUES($2,($1->>'user')::uuid,$1->'pickup',$1->'destination',$1->'route',$1->'options',(now() AT TIME ZONE 'UTC')+interval '10 minutes',(now() AT TIME ZONE 'UTC')) RETURNING '{}'::jsonb AS data",json!({"user":passenger,"pickup":place,"destination":destination,"route":route,"options":[{"option":{"id":"go"},"fare":fare}]}),id).await.unwrap();
    id
}
/// Create a booking through the public service rather than bypassing its validation/quote snapshot.
async fn booking(s: Arc<AppState>, passenger: Uuid, hours: i64) -> Value {
    let estimate_id = estimate(&s, passenger).await;
    let at = Utc::now() + Duration::hours(hours);
    let payload = || ScheduleRideDto {
        estimate_id,
        option_id: "go".into(),
        scheduled_at: at,
        timezone: "Africa/Lagos".into(),
    };
    let first = create(s.clone(), claims(passenger, RoleType::User), payload())
        .await
        .unwrap();
    let retry = create(s, claims(passenger, RoleType::User), payload())
        .await
        .unwrap();
    assert_eq!(
        first["id"], retry["id"],
        "identical request must return its existing booking"
    );
    first
}
/// Read the version presented on a driver's current request card.
async fn latest_version(s: &AppState, id: Uuid) -> i32 {
    let mut db = connection(s).await.unwrap();
    detail_query(&mut db, id).await.unwrap()["version"]
        .as_i64()
        .unwrap() as i32
}
/// Run one maintenance transaction exactly as the production periodic worker does.
async fn maintain(s: &AppState, id: Uuid) {
    let mut db = connection(s).await.unwrap();
    db.transaction::<(), ModuleError, _>(|db| Box::pin(scheduled_worker::maintain_booking(db, id)))
        .await
        .unwrap();
}

struct FakeSmtp(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl crate::mailer::worker::FallbackSender for FakeSmtp {
    /// Record a fake SMTP submission without making an external network connection.
    async fn submit(&self, _: &crate::mailer::worker::EmailJob) -> Result<(), String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
/// Exercise real concurrent locks, reopening, approaching broadcasts, activation, fare locking, and provider failover.
#[tokio::test]
#[ignore = "requires SCHEDULED_TEST_DATABASE_URL and SCHEDULED_TEST_REDIS_URL pointing to disposable migrated services"]
async fn scheduled_workflow_and_email_failover() {
    let manager = crate::AsyncDieselConnectionManager::<crate::AsyncPgConnection>::new(
        std::env::var("SCHEDULED_TEST_DATABASE_URL").unwrap(),
    );
    let pool = Arc::new(
        bb8::Pool::builder()
            .max_size(8)
            .build(manager)
            .await
            .unwrap(),
    );
    let redis = redis::Client::open(std::env::var("SCHEDULED_TEST_REDIS_URL").unwrap()).unwrap();
    let redis = redis::aio::ConnectionManager::new(redis).await.unwrap();
    let (mailer, _rx) = async_channel::bounded(1);
    let (logger, _logs) = async_channel::bounded(1);
    let s = Arc::new(AppState {
        pool,
        mailer,
        logger,
        dispatcher: Arc::new(dispatcher::RideDispatcher::new(redis)),
        ws_registry: Arc::new(super::realtime::WsRegistry::new()),
    });
    let p = user(&s, false).await;
    let p2 = user(&s, false).await;
    let p3 = user(&s, false).await;
    let d1 = user(&s, true).await;
    let d2 = user(&s, true).await;
    let b = booking(s.clone(), p, 2).await;
    let id: Uuid = serde_json::from_value(b["id"].clone()).unwrap();
    let locked = b["total"].clone();
    assert_eq!(b["fareLocked"], true);
    assert_eq!(b["fare"]["isEstimate"], false);
    assert_eq!(b["fare"]["range"]["low"], locked);
    assert_eq!(b["fare"]["range"]["high"], locked);
    serde_json::from_value::<ScheduledRideDto>(b.clone()).unwrap();
    let board = list(
        s.clone(),
        claims(d1, RoleType::Driver),
        ScheduledQuery::default(),
        true,
    )
    .await
    .unwrap();
    assert!(
        board["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == b["id"])
    );
    assert!(!board.to_string().contains("phone"));
    assert!(
        detail(s.clone(), claims(p2, RoleType::User), id)
            .await
            .is_err()
    );
    let mut db = connection(&s).await.unwrap();
    let message=query(&mut db,"SELECT jsonb_build_object('body',body) AS data FROM email_outbox WHERE aggregate_id=$2 AND $1 IS NOT NULL ORDER BY sequence_id LIMIT 1",json!({}),id).await.unwrap();
    assert!(!message["body"].as_str().unwrap().contains("minor units"));
    assert!(
        !message["body"]
            .as_str()
            .unwrap()
            .contains("scheduled.opened")
    );
    drop(db);

    let (a, z) = tokio::join!(
        accept(
            s.clone(),
            claims(d1, RoleType::Driver),
            id,
            latest_version(&s, id).await
        ),
        accept(
            s.clone(),
            claims(d2, RoleType::Driver),
            id,
            latest_version(&s, id).await
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(z.is_ok()), 1);
    let winner = if a.is_ok() { d1 } else { d2 };
    let loser = if a.is_err() {
        a.unwrap_err()
    } else {
        z.unwrap_err()
    };
    assert!(matches!(loser, ModuleError::Conflict(_)));
    let mut db = connection(&s).await.unwrap();
    let guarded=query(&mut db,"INSERT INTO rides(id,passenger_id,driver_id,status,option_id,option_snapshot,estimate_id,pickup,destination,route,fare,currency,fare_total_minor,trip_pin) SELECT ($1->>'ride')::uuid,($1->>'passenger')::uuid,($1->>'driver')::uuid,'DRIVER_ASSIGNED',option_id,option_snapshot,estimate_id,pickup,destination,jsonb_set(route,'{durationSeconds}','7200'),fare,currency,fare_total_minor,'1234' FROM scheduled_rides WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"ride":Uuid::now_v7(),"passenger":p2,"driver":winner}),id).await;
    assert!(
        guarded.is_err(),
        "immediate trips must honor scheduled driver commitments"
    );
    drop(db);
    let c1 = booking(s.clone(), p2, 4).await;
    let c2 = booking(s.clone(), p3, 4).await;
    let id1 = serde_json::from_value(c1["id"].clone()).unwrap();
    let id2 = serde_json::from_value(c2["id"].clone()).unwrap();
    let (a, z) = tokio::join!(
        accept(
            s.clone(),
            claims(winner, RoleType::Driver),
            id1,
            latest_version(&s, id1).await
        ),
        accept(
            s.clone(),
            claims(winner, RoleType::Driver),
            id2,
            latest_version(&s, id2).await
        )
    );
    assert_eq!(
        usize::from(a.is_ok()) + usize::from(z.is_ok()),
        1,
        "driver must not reserve two overlapping bookings"
    );
    let edited = reschedule(
        s.clone(),
        claims(p2, RoleType::User),
        id1,
        RescheduleRideDto {
            scheduled_at: Utc::now() + Duration::hours(6),
            timezone: "Africa/Lagos".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(edited["status"], "OPEN");
    assert!(
        matches!(
            accept(s.clone(), claims(winner, RoleType::Driver), id1, 1).await,
            Err(ModuleError::Conflict(_))
        ),
        "stale cards must not accept a changed pickup time"
    );
    assert_eq!(edited["total"], c1["total"]);
    let reopened = cancel(
        s.clone(),
        claims(winner, RoleType::Driver),
        id,
        ScheduledCancellationDto {
            reason: "Driver unavailable".into(),
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(reopened["status"], "OPEN");
    assert_eq!(reopened["total"], locked);
    let expiry = booking(s.clone(), p3, 8).await;
    let expiry_id = serde_json::from_value(expiry["id"].clone()).unwrap();
    let mut db = connection(&s).await.unwrap();
    query(&mut db,"UPDATE scheduled_rides SET scheduled_at=(now() AT TIME ZONE 'UTC')-interval '1 second' WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),expiry_id).await.unwrap();
    drop(db);
    maintain(&s, expiry_id).await;
    assert_eq!(
        detail(s.clone(), claims(p3, RoleType::User), expiry_id)
            .await
            .unwrap()["status"],
        "EXPIRED"
    );
    let mut db = connection(&s).await.unwrap();
    query(&mut db,"UPDATE scheduled_rides SET scheduled_at=(now() AT TIME ZONE 'UTC')+interval '29 minutes',next_broadcast_at=(now() AT TIME ZONE 'UTC')-interval '1 second' WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),id).await.unwrap();
    drop(db);
    maintain(&s, id).await;
    maintain(&s, id).await;
    let mut db = connection(&s).await.unwrap();
    let n=query(&mut db,"SELECT jsonb_build_object('n',count(*)) AS data FROM scheduled_events WHERE scheduled_ride_id=$2 AND kind='scheduled.matching_reminder' AND $1 IS NOT NULL",json!({}),id).await.unwrap();
    assert_eq!(n["n"], 1);
    query(&mut db,"UPDATE scheduled_rides SET scheduled_at=(now() AT TIME ZONE 'UTC')+interval '14 minutes' WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),id).await.unwrap();
    drop(db);
    accept(
        s.clone(),
        claims(winner, RoleType::Driver),
        id,
        latest_version(&s, id).await,
    )
    .await
    .unwrap();
    maintain(&s, id).await;
    let activated = detail(s.clone(), claims(p, RoleType::User), id)
        .await
        .unwrap();
    assert_eq!(activated["status"], "ACTIVATED");
    let first_ride = serde_json::from_value(activated["rideId"].clone()).unwrap();
    scheduled_worker::tick(&s).await.unwrap();
    assert_eq!(
        s.ws_registry.get_driver_active_ride(&winner),
        Some(first_ride)
    );

    maintain(&s, id).await;
    assert_eq!(
        activated["rideId"],
        detail(s.clone(), claims(p, RoleType::User), id)
            .await
            .unwrap()["rideId"]
    );
    super::services::drivers::driver_cancel_ride(
        s.clone(),
        claims(winner, RoleType::Driver),
        first_ride,
        CancelRideDto {
            reason: Some("Cannot make pickup".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        detail(s.clone(), claims(p, RoleType::User), id)
            .await
            .unwrap()["status"],
        "OPEN"
    );
    accept(
        s.clone(),
        claims(winner, RoleType::Driver),
        id,
        latest_version(&s, id).await,
    )
    .await
    .unwrap();
    maintain(&s, id).await;
    let active = detail(s.clone(), claims(p, RoleType::User), id)
        .await
        .unwrap();
    let ride: Uuid = serde_json::from_value(active["rideId"].clone()).unwrap();
    assert_ne!(ride, first_ride);
    super::services::drivers::driver_arrived(s.clone(), claims(winner, RoleType::Driver), ride)
        .await
        .unwrap();
    super::services::drivers::start_ride(
        s.clone(),
        claims(winner, RoleType::Driver),
        ride,
        StartRideDto { pin: None },
    )
    .await
    .unwrap();
    let complete =
        super::services::drivers::complete_ride(s.clone(), claims(winner, RoleType::Driver), ride)
            .await
            .unwrap();
    assert_eq!(serde_json::to_value(complete.fare.total).unwrap(), locked);
    assert_eq!(
        detail(s.clone(), claims(p, RoleType::User), id)
            .await
            .unwrap()["status"],
        "COMPLETED"
    );
    let mut db = connection(&s).await.unwrap();
    query(&mut db,"UPDATE email_outbox SET sent_at=(now() AT TIME ZONE 'UTC') WHERE recipient_id=ANY(ARRAY(SELECT jsonb_array_elements_text($1)::uuid)) RETURNING '{}'::jsonb AS data",json!([p,p2,p3,d1,d2]),id).await.unwrap();
    drop(db);
    // A local fake API verifies stable idempotency headers across uncertain primary submissions.
    let status = Arc::new(AtomicU16::new(500));
    let requests = Arc::new(AtomicUsize::new(0));
    let api_status = status.clone();
    let api_requests = requests.clone();
    let app = axum::Router::new().route(
        "/emails",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                let st = api_status.clone();
                let count = api_requests.clone();
                async move {
                    assert!(
                        headers["idempotency-key"]
                            .to_str()
                            .unwrap()
                            .starts_with("scheduled-email/")
                    );
                    assert_eq!(body["text"], "Test email");
                    count.fetch_add(1, Ordering::SeqCst);
                    (
                        axum::http::StatusCode::from_u16(st.load(Ordering::SeqCst)).unwrap(),
                        axum::Json(json!({"id":"fake-provider-id"})),
                    )
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let smtp = Arc::new(AtomicUsize::new(0));
    let delivery = crate::mailer::worker::Delivery {
        primary: Some(crate::mailer::resend::Resend::test_endpoint(format!(
            "http://{address}/emails"
        ))),
        fallback: Some(Arc::new(FakeSmtp(smtp.clone()))),
    };
    for (case, http_status, tries) in [
        ("uncertain", 500, 3),
        ("rejected", 400, 1),
        ("accepted", 200, 1),
    ] {
        status.store(http_status, Ordering::SeqCst);
        let job = Uuid::now_v7();
        let mut db = connection(&s).await.unwrap();
        query(&mut db,"INSERT INTO email_outbox(id,aggregate_id,event_key,recipient_id,recipient,subject,body) VALUES($2,($1->>'aggregate')::uuid,$2::text,($1->>'user')::uuid,'test@example.invalid','Test','Test email') RETURNING '{}'::jsonb AS data",json!({"aggregate":id,"user":p}),job).await.unwrap();
        drop(db);
        for attempt in 0..tries {
            assert!(
                crate::mailer::worker::deliver_one(&s, &delivery)
                    .await
                    .unwrap()
            );
            let mut db = connection(&s).await.unwrap();
            let state = query(
                &mut db,
                "SELECT to_jsonb(e) AS data FROM email_outbox e WHERE id=$2 AND $1 IS NOT NULL",
                json!({}),
                job,
            )
            .await
            .unwrap();
            if attempt + 1 < tries {
                assert!(state["sent_at"].is_null());
                query(&mut db,"UPDATE email_outbox SET next_attempt_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),job).await.unwrap();
            } else {
                assert!(!state["sent_at"].is_null(), "{case}");
            }
        }
    }
    assert_eq!(smtp.load(Ordering::SeqCst), 2);
    assert_eq!(requests.load(Ordering::SeqCst), 5);
    server.abort();
}
