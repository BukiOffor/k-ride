//! Restart-safe booking maintenance and leased WebSocket announcements; network I/O happens after commit.
use crate::vrp::models::scheduled::ScheduledJsonRow;
use crate::vrp::realtime::{OutgoingWsMessage, WsEventFrame};
use crate::vrp::services::scheduled::{connection, detail_query, lock_users, query, wake};
use crate::{AppState, Connection, ModuleError};
use diesel::sql_types::{Jsonb, Uuid as SqlUuid};
use diesel_async::{AsyncConnection, RunQueryDsl};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Start a bounded periodic scan; duplicate instances serialize changes through user and booking locks.
pub fn start(s: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = tick(&s).await {
                tracing::error!(error=%e,"Scheduled ride worker failed; retrying next tick");
            }
        }
    })
}
/// Process one bounded batch of due bookings, then deliver pending broadcasts from the durable event queue.
pub async fn tick(s: &Arc<AppState>) -> Result<(), ModuleError> {
    let mut db = connection(s).await?;
    let due=query(&mut db,"SELECT COALESCE(jsonb_agg(id),'[]'::jsonb) AS data FROM (SELECT id FROM scheduled_rides WHERE (status='OPEN' AND (scheduled_at<=(now() AT TIME ZONE 'UTC') OR next_broadcast_at<=(now() AT TIME ZONE 'UTC'))) OR (status='RESERVED' AND (scheduled_at<=(now() AT TIME ZONE 'UTC')+interval '15 minutes' OR (NOT reminder_sent AND scheduled_at<=(now() AT TIME ZONE 'UTC')+interval '1 hour'))) ORDER BY scheduled_at,id LIMIT 50) due WHERE $1 IS NOT NULL AND $2 IS NOT NULL",json!({}),Uuid::nil()).await?;
    drop(db);
    for id in due.as_array().into_iter().flatten() {
        let id: Uuid = serde_json::from_value(id.clone())?;
        let mut db = connection(s).await?;
        if let Err(e) = db
            .transaction::<(), ModuleError, _>(|db| Box::pin(maintain_booking(db, id)))
            .await
        {
            tracing::warn!(booking=%id,error=%e,"Scheduled booking deferred");
        }
    }
    deliver_events(s).await?;
    wake(s);
    Ok(())
}
/// Under canonical locks, expire open requests, send one reminder, rebroadcast approaching pickups or activate a reservation.
pub(crate) async fn maintain_booking(db: &mut Connection<'_>, id: Uuid) -> Result<(), ModuleError> {
    let v = detail_query(db, id).await?;
    let passenger: Uuid = serde_json::from_value(v["passengerId"].clone())?;
    let driver: Option<Uuid> = serde_json::from_value(v["driverId"].clone())?;
    lock_users(db, passenger, driver).await?;
    let row=query(db,"SELECT to_jsonb(s) AS data FROM scheduled_rides s WHERE id=$2 AND $1 IS NOT NULL FOR UPDATE",json!({}),id).await?;
    if row["driver_id"] != json!(driver) {
        return Ok(());
    }
    if row["status"] == "OPEN" {
        query(db,r#"WITH changed AS (UPDATE scheduled_rides SET status=CASE WHEN scheduled_at<=(now() AT TIME ZONE 'UTC') THEN 'EXPIRED' ELSE status END,version=version+CASE WHEN scheduled_at<=(now() AT TIME ZONE 'UTC') THEN 1 ELSE 0 END,updated_at=(now() AT TIME ZONE 'UTC'),next_broadcast_at=scheduled_next_broadcast(scheduled_at) WHERE id=$2 AND (scheduled_at<=(now() AT TIME ZONE 'UTC') OR next_broadcast_at<=(now() AT TIME ZONE 'UTC')) RETURNING *) SELECT jsonb_build_object('emitted',COALESCE((SELECT CASE WHEN status='OPEN' THEN scheduled_emit(id,'scheduled.matching_reminder',true) IS NULL ELSE true END FROM changed),false)) AS data WHERE $1 IS NOT NULL"#,json!({}),id).await?;
    } else if row["status"] == "RESERVED" {
        // Reminder bookkeeping does not change the reservation version or fare.
        query(db,"WITH changed AS (UPDATE scheduled_rides SET reminder_sent=true WHERE id=$2 AND NOT reminder_sent AND scheduled_at<=(now() AT TIME ZONE 'UTC')+interval '1 hour' RETURNING id) SELECT jsonb_build_object('reminder',COALESCE((SELECT scheduled_emit(id,'scheduled.reminder') IS NULL FROM changed),false)) AS data WHERE $1 IS NOT NULL",json!({}),id).await?;
        activate_locked(db, id, passenger, driver).await?;
    }
    Ok(())
}
/// Create exactly one normal assigned ride within fifteen minutes of pickup; busy participants are retried for ten minutes.
pub(crate) async fn activate_locked(
    db: &mut Connection<'_>,
    id: Uuid,
    passenger: Uuid,
    driver: Option<Uuid>,
) -> Result<(), ModuleError> {
    let guard=query(db,"SELECT jsonb_build_object('due',scheduled_at<=(now() AT TIME ZONE 'UTC')+interval '15 minutes','expired',scheduled_at<(now() AT TIME ZONE 'UTC')-interval '10 minutes','eligible',scheduled_driver_eligible(driver_id,option_snapshot) AND EXISTS(SELECT 1 FROM users WHERE id=passenger_id AND is_active),'future',scheduled_at>(now() AT TIME ZONE 'UTC'),'busy',EXISTS(SELECT 1 FROM rides r WHERE r.status NOT IN ('COMPLETED','CANCELLED') AND (r.passenger_id=($1->>'passenger')::uuid OR r.driver_id=($1->>'driver')::uuid))) AS data FROM scheduled_rides WHERE id=$2",json!({"passenger":passenger,"driver":driver}),id).await?;
    if guard["expired"] == true || guard["eligible"] != true {
        query(db,"UPDATE scheduled_rides SET status=CASE WHEN NOT ($1->>'eligible')::boolean AND scheduled_at>(now() AT TIME ZONE 'UTC') THEN 'OPEN' ELSE 'EXPIRED' END,driver_id=CASE WHEN NOT ($1->>'eligible')::boolean AND scheduled_at>(now() AT TIME ZONE 'UTC') THEN NULL ELSE driver_id END,version=version+1,next_broadcast_at=scheduled_next_broadcast(scheduled_at),cancellation_reason='Assigned driver unavailable',updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",guard,id).await?;
        return Ok(());
    }
    if guard["due"] != true || guard["busy"] == true {
        return Ok(());
    }
    let ride_id = Uuid::now_v7();
    let pin = rand::random_range(1000..=9999).to_string();
    // The deferred FK permits linking first so the live-ride overlap guard can recognize this reservation.
    query(db,"UPDATE scheduled_rides SET ride_id=($1->>'ride')::uuid,status='ACTIVATED',version=version+1,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"ride":ride_id}),id).await?;
    query(db,r#"INSERT INTO rides(id,passenger_id,driver_id,status,version,option_id,option_snapshot,estimate_id,pickup,destination,route,fare,currency,fare_total_minor,payment_method,payment_status,trip_pin,assigned_at)
 SELECT ($1->>'ride')::uuid,passenger_id,driver_id,'DRIVER_ASSIGNED',1,option_id,jsonb_set(option_snapshot,'{waiting_per_minute_minor}','0'::jsonb),estimate_id,pickup,destination,route,fare,currency,fare_total_minor,'cash','pending',$1->>'pin',(now() AT TIME ZONE 'UTC') FROM scheduled_rides WHERE id=$2 RETURNING jsonb_build_object('id',id) AS data"#,json!({"ride":ride_id,"pin":pin}),id).await?;
    query(db,"INSERT INTO driver_availability(driver_id,status,updated_at) SELECT driver_id,'ON_TRIP',(now() AT TIME ZONE 'UTC') FROM scheduled_rides WHERE id=$2 AND $1 IS NOT NULL ON CONFLICT(driver_id) DO UPDATE SET status='ON_TRIP',updated_at=EXCLUDED.updated_at RETURNING '{}'::jsonb AS data",json!({}),id).await?;
    Ok(())
}
/// Lease an event, publish only to authorized booking channels/eligible driver mailboxes, then acknowledge its lease.
async fn deliver_events(s: &Arc<AppState>) -> Result<(), ModuleError> {
    for _ in 0..50 {
        let token = Uuid::now_v7();
        let mut db = connection(s).await?;
        let v=query(&mut db,"WITH candidate AS (SELECT id FROM scheduled_events WHERE delivered_at IS NULL AND (lease_until IS NULL OR lease_until<(now() AT TIME ZONE 'UTC')) ORDER BY sequence_id LIMIT 1 FOR UPDATE SKIP LOCKED), claimed AS (UPDATE scheduled_events SET lease_token=$2,lease_until=(now() AT TIME ZONE 'UTC')+interval '90 seconds' WHERE id IN(SELECT id FROM candidate) RETURNING *) SELECT COALESCE((SELECT to_jsonb(claimed) FROM claimed),'null'::jsonb) AS data WHERE $1 IS NOT NULL",json!({}),token).await?;
        if v.is_null() {
            break;
        }
        let booking: Uuid = serde_json::from_value(v["scheduled_ride_id"].clone())?;
        let current = detail_query(&mut db, booking).await?;
        // Old broadcasts are suppressed after acceptance/cancellation, but participant state-change events remain ordered hints.
        let is_open = current["status"] == "OPEN";
        let ids: Vec<Uuid> = if v["broadcast"] == true && is_open {
            let connected: Vec<Uuid> = s.ws_registry.drivers.iter().map(|d| *d.key()).collect();
            let rows=diesel::sql_query("SELECT jsonb_build_object('id',u.id) AS data FROM users u JOIN scheduled_rides b ON b.id=$2 WHERE u.id=ANY(ARRAY(SELECT jsonb_array_elements_text($1)::uuid)) AND u.id<>b.passenger_id AND scheduled_driver_eligible(u.id,b.option_snapshot)").bind::<Jsonb,_>(json!(connected)).bind::<SqlUuid,_>(booking).load::<ScheduledJsonRow>(&mut db).await?;
            rows.into_iter()
                .map(|r| serde_json::from_value(r.data["id"].clone()))
                .collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        let event_id: Uuid = serde_json::from_value(v["id"].clone())?;
        drop(db);
        let frame = WsEventFrame {
            id: event_id.to_string(),
            ride_id: current["rideId"].as_str().and_then(|x| x.parse().ok()),
            seq: 0,
            ride_version: None,
            occurred_at: chrono::Utc::now().to_rfc3339(),
            payload: json!({"type":v["kind"],"scheduledRideId":booking,"booking":current}),
        };
        if let Some(channel) = s
            .ws_registry
            .admin_channels
            .get(&format!("scheduled:{booking}"))
        {
            let _ = channel.send(frame.clone());
        }
        let mut recipients = ids;
        if let Some(old) = v["driver_ids"].as_array() {
            for x in old {
                if let Ok(id) = serde_json::from_value(x.clone()) {
                    recipients.push(id);
                }
            }
        }
        recipients.sort_unstable();
        recipients.dedup();
        for driver in recipients {
            // try_send avoids holding the worker on a slow WebSocket consumer; REST remains the durable board.
            if let Some(tx) = s.ws_registry.drivers.get(&driver) {
                let _ = tx.try_send(OutgoingWsMessage::Event {
                    event: frame.clone(),
                });
            }
        }
        if let Some(driver) = current["driverId"]
            .as_str()
            .and_then(|x| x.parse::<Uuid>().ok())
        {
            if current["status"] == "ACTIVATED" {
                if let Some(ride) = frame.ride_id {
                    s.ws_registry.set_driver_active_ride(driver, Some(ride));
                    s.ws_registry.set_driver_online(driver, false);
                    let mut dispatcher = (*s.dispatcher).clone();
                    if let Err(e) = dispatcher.remove_driver(&driver.to_string()).await {
                        tracing::warn!(error=%e,"Scheduled driver spatial cleanup failed");
                    }
                }
            }
        }
        let mut db = connection(s).await?;
        query(&mut db,"UPDATE scheduled_events SET delivered_at=(now() AT TIME ZONE 'UTC'),lease_until=NULL WHERE id=($1->>'event')::uuid AND lease_token=$2 RETURNING '{}'::jsonb AS data",json!({"event":event_id}),token).await?;
    }
    Ok(())
}
