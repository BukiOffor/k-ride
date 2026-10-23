use super::dto;
use super::dto::*;
use super::services::people::requests_restricted;
use super::services::safety::incident_data;
use super::services::{applications, reports, *};
use crate::auth::jwt::Claims;
use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;
#[test]
/// Verify role restrictions and paging limits independently of a database.
fn role_and_paging_contracts() {
    for role in [
        crate::users::RoleType::User,
        crate::users::RoleType::Driver,
        crate::users::RoleType::RouteCoordinator,
    ] {
        assert!(
            require_admin(&Claims {
                user_id: Uuid::now_v7(),
                role,
                exp: usize::MAX
            })
            .is_err()
        );
    }
    for role in [crate::users::RoleType::Admin, crate::users::RoleType::Root] {
        assert!(
            require_admin(&Claims {
                user_id: Uuid::now_v7(),
                role,
                exp: usize::MAX
            })
            .is_ok()
        );
    }
    assert_eq!(Filters::default().params().unwrap()["limit"], 25);
    assert!(
        Filters {
            limit: Some(101),
            ..Default::default()
        }
        .params()
        .is_err()
    );
    assert!(
        Filters {
            limit: Some(0),
            ..Default::default()
        }
        .params()
        .is_err()
    );
}
/// Explicitly opt in with a disposable database that has all migrations applied.
#[tokio::test]
#[ignore = "requires ADMIN_TEST_DATABASE_URL pointing to a migrated disposable database"]
async fn database_projection_and_safety_contracts() {
    use diesel_async::SimpleAsyncConnection;
    let url = std::env::var("ADMIN_TEST_DATABASE_URL").expect("ADMIN_TEST_DATABASE_URL");
    let manager = crate::AsyncDieselConnectionManager::<crate::AsyncPgConnection>::new(url);
    let pool = bb8::Pool::builder()
        .max_size(1)
        .build(manager)
        .await
        .unwrap();
    let mut db = pool.get().await.unwrap();
    db.batch_execute("BEGIN").await.unwrap();
    let admin = Uuid::now_v7();
    let driver = Uuid::now_v7();
    let passenger = Uuid::now_v7();
    let car = Uuid::now_v7();
    let ride = Uuid::now_v7();
    let alert = Uuid::now_v7();
    for (id, role) in [(admin, "Admin"), (driver, "Driver"), (passenger, "User")] {
        query(&mut db,"INSERT INTO users(id,first_name,last_name,email,username,phone,address,country,role,city,password_hash) VALUES($2,'Test','Person',($2::text)||'@test.invalid',$2::text,$2::text,'address','NG',$1->>'role','Test','unused') RETURNING '{}'::jsonb AS data",json!({"role":role}),id).await.unwrap();
    }
    query(&mut db,"INSERT INTO cars(id,license_plate,capacity,car_type,car_tier,car_size,color,brand,model,year,vehicle_photo,vehicle_registration,insurance_certificate,roadworthiness_certificate) VALUES($2,'ADMIN-TEST',4,'sedan','standard','medium','blue','Test','Car',2026,'body','body','body','body') RETURNING '{}'::jsonb AS data",json!({}),car).await.unwrap();
    query(&mut db,"INSERT INTO driver_profiles(id,user_id,car_id,nin_slip,profile_photo,is_koinonia_worker,licence_number,licence_class,licence_expiry_date,licence_photo,track,status) VALUES($2,$2,($1->>'car')::uuid,'body','body',true,'test','B',CURRENT_DATE+365,'body','Professional','Pending') RETURNING '{}'::jsonb AS data",json!({"car":car}),driver).await.unwrap();
    let docs = query(&mut db, applications::DOCUMENT_SQL, json!({}), driver)
        .await
        .unwrap();
    assert_eq!(docs.as_array().unwrap().len(), 7);
    assert!(
        docs.as_array()
            .unwrap()
            .iter()
            .all(|d| d["status"] == "UPLOADED")
    );
    let claims = Claims {
        user_id: admin,
        role: crate::users::RoleType::Admin,
        exp: usize::MAX,
    };
    let approval: Decision = serde_json::from_value(json!({"decision":"APPROVE"})).unwrap();
    let blocked = applications::apply_decision(&mut db, &claims, driver, &approval)
        .await
        .unwrap_err();
    assert_eq!(blocked.0, StatusCode::CONFLICT);
    for d in docs.as_array().unwrap() {
        query(&mut db,"INSERT INTO admin_document_reviews(user_id,document_type,status,reviewer_id) VALUES($2,$1->>'doc','VERIFIED',($1->>'reviewer')::uuid) RETURNING '{}'::jsonb AS data",json!({"doc":d["type"],"reviewer":admin}),driver).await.unwrap();
    }
    let approved = applications::apply_decision(&mut db, &claims, driver, &approval)
        .await
        .unwrap();
    assert_eq!(approved["status"], "APPROVED");
    let car_status=query(&mut db,"SELECT jsonb_build_object('verified',is_verified) AS data FROM cars WHERE id=$2 AND $1 IS NOT NULL",json!({}),car).await.unwrap();
    assert_eq!(car_status["verified"], true);
    query(&mut db,"UPDATE driver_profiles SET licence_photo='replacement',status='SUBMITTED' WHERE user_id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),driver).await.unwrap();
    let blocked = applications::apply_decision(&mut db, &claims, driver, &approval)
        .await
        .unwrap_err();
    assert_eq!(blocked.0, StatusCode::CONFLICT);
    let people = query(
        &mut db,
        &page_sql("admin_people_view", "role='Driver' AND id=$2"),
        Filters {
            limit: Some(1),
            ..Default::default()
        }
        .params()
        .unwrap(),
        driver,
    )
    .await
    .unwrap();
    assert_eq!(people["items"][0]["verificationStatus"], "SUBMITTED");
    serde_json::from_value::<dto::Page<dto::PersonSummary>>(people.clone()).unwrap();
    assert!(!people.to_string().contains("base64"));
    query(&mut db,"INSERT INTO rides(id,passenger_id,driver_id,status,option_id,option_snapshot,estimate_id,pickup,destination,route,fare,currency,fare_total_minor,trip_pin) VALUES($2,($1->>'passenger')::uuid,($1->>'driver')::uuid,'IN_PROGRESS','go','{}',$2,'{\"label\":\"Pickup\"}','{\"label\":\"Destination\"}','{}','{}','NGN',100000,'1234') RETURNING '{}'::jsonb AS data",json!({"passenger":passenger,"driver":driver}),ride).await.unwrap();
    query(&mut db,"INSERT INTO safety_events(id,ride_id,reporter_id,reporter_role,kind,event_type,status,occurred_at) VALUES($2,($1->>'ride')::uuid,($1->>'passenger')::uuid,'passenger','sos','threat','sent',(now() AT TIME ZONE 'UTC')) RETURNING '{}'::jsonb AS data",json!({"ride":ride,"passenger":passenger}),alert).await.unwrap();
    let open = incident_data(&mut db, alert).await.unwrap();
    serde_json::from_value::<dto::Incident>(open.clone()).unwrap();
    assert_eq!(open["category"], "SOS_ACTIVATION");
    assert_eq!(open["severity"], "CRITICAL");
    assert_eq!(open["participants"].as_array().unwrap().len(), 2);
    assert!(
        open["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("phone").is_some())
    );
    query(&mut db,"UPDATE admin_incidents SET status='RESOLVED' WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),alert).await.unwrap();
    let closed = incident_data(&mut db, alert).await.unwrap();
    assert!(
        closed["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("phone").is_none())
    );
    let notifications=query(&mut db,"SELECT jsonb_build_object('n',count(*)) AS data FROM ride_notifications WHERE recipient_id IN(($1->>'driver')::uuid,$2)",json!({"driver":driver}),passenger).await.unwrap();
    assert_eq!(notifications["n"], 2);
    let report = query(&mut db, reports::REPORT_SQL, json!({"days":7}), admin)
        .await
        .unwrap();
    assert_eq!(report.as_array().unwrap().len(), 6);
    let series = query(&mut db, reports::SERIES_SQL, json!({"days":7}), admin)
        .await
        .unwrap();
    assert_eq!(series["adoption"].as_array().unwrap().len(), 7);
    let page = query(
        &mut db,
        &page_sql("admin_people_view", "true"),
        Filters {
            limit: Some(1),
            ..Default::default()
        }
        .params()
        .unwrap(),
        admin,
    )
    .await
    .unwrap();
    let cursor = serde_json::from_value(page["nextCursor"].clone()).unwrap();
    let next = query(
        &mut db,
        &page_sql("admin_people_view", "true"),
        Filters {
            limit: Some(1),
            cursor: Some(cursor),
            ..Default::default()
        }
        .params()
        .unwrap(),
        admin,
    )
    .await
    .unwrap();
    assert_ne!(page["items"][0]["userId"], next["items"][0]["userId"]);
    query(&mut db,"INSERT INTO admin_user_state(user_id,restricted,restriction_reason) VALUES($2,true,COALESCE($1->>'reason','test')) ON CONFLICT(user_id) DO UPDATE SET restricted=true RETURNING '{}'::jsonb AS data",json!({}),passenger).await.unwrap();
    assert!(requests_restricted(&mut db, passenger).await.unwrap());

    audit(&mut db, admin, driver, "verification.test", json!({}))
        .await
        .unwrap();
    db.batch_execute("ROLLBACK").await.unwrap();
}

/// Reject malformed admin scheduled filters before acquiring a database connection.
#[test]
fn scheduled_filter_contracts() {
    assert_eq!(
        ScheduledRideFilters::default().params().unwrap()["limit"],
        25
    );
    for value in [0, 101] {
        assert!(
            ScheduledRideFilters {
                limit: Some(value),
                ..Default::default()
            }
            .params()
            .is_err()
        );
    }
    assert!(
        ScheduledRideFilters {
            status: Some("open".into()),
            ..Default::default()
        }
        .params()
        .is_err()
    );
    let now = chrono::Utc::now();
    assert!(
        ScheduledRideFilters {
            from: Some(now),
            to: Some(now),
            ..Default::default()
        }
        .params()
        .is_err()
    );
    assert!(
        serde_json::from_value::<AssignScheduledDriver>(
            json!({"driverId":Uuid::now_v7(),"version":1,"force":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<AssignScheduledDriver>(json!({"driverId":Uuid::now_v7()}))
            .is_err()
    );
}
