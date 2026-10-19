//! Shared authorization, database, audit and validation services.
use crate::admin::{dto::*, errors::*, model::Row};
use crate::{AppState, Connection, ModuleError, auth::jwt::Claims};
use axum::http::StatusCode;
use diesel::sql_types::{Jsonb, Uuid as SqlUuid};
use diesel_async::{AsyncConnection, RunQueryDsl};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;
pub mod account;
pub mod applications;
pub mod people;
pub mod quality;
pub mod reports;
pub mod rides;
pub mod safety;

/// Reject callers whose JWT role is neither Admin nor Root.
pub(crate) fn require_admin(c: &Claims) -> Result<(), AdminError> {
    if matches!(
        c.role,
        crate::users::RoleType::Admin | crate::users::RoleType::Root
    ) {
        Ok(())
    } else {
        Err(AdminError(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Admin or Root role required".into(),
        ))
    }
}

/// Execute a parameterized JSON projection using the existing pooled connection.
pub(crate) async fn query(
    conn: &mut Connection<'_>,
    sql: &str,
    p: Value,
    id: Uuid,
) -> Result<Value, AdminError> {
    Ok(diesel::sql_query(sql)
        .bind::<Jsonb, _>(p)
        .bind::<SqlUuid, _>(id)
        .get_result::<Row>(conn)
        .await?
        .data)
}

/// Acquire a database connection and map pool failures to an unavailable error.
pub(crate) async fn connection(state: &AppState) -> Result<Connection<'_>, AdminError> {
    state.pool.get().await.map_err(|e| {
        tracing::error!(error=%e,"Admin pool unavailable");
        AdminError(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "Database unavailable".into(),
        )
    })
}

/// Build a keyset-paginated projection with a filtered total count.
pub(crate) fn page_sql(source: &str, predicate: &str) -> String {
    format!(
        "WITH filtered AS (SELECT id,data FROM {source} WHERE ({predicate}) AND $2 IS NOT NULL), page AS (SELECT id,data FROM filtered WHERE ($1->>'cursor' IS NULL OR id < ($1->>'cursor')::uuid) ORDER BY id DESC LIMIT (($1->>'limit')::int+1)), visible AS (SELECT * FROM page ORDER BY id DESC LIMIT ($1->>'limit')::int) SELECT jsonb_build_object('items',COALESCE((SELECT jsonb_agg(data ORDER BY id DESC) FROM visible),'[]'::jsonb),'total',(SELECT count(*) FROM filtered),'nextCursor',CASE WHEN (SELECT count(*) FROM page)>($1->>'limit')::int THEN (SELECT id FROM visible ORDER BY id LIMIT 1) END) AS data"
    )
}

/// Persist the domain event and activity log entry within the caller’s transaction.
pub(crate) async fn audit(
    db: &mut Connection<'_>,
    actor: Uuid,
    target: Uuid,
    event: &str,
    detail: Value,
) -> Result<(), AdminError> {
    let activity = match event.split('.').next().unwrap_or("") {
        "sos" => "RideSafetyAction",
        "incident" => "IncidentAction",
        "verification" => "DriverVerificationAction",
        "inspection" => "DriverInspectionAction",
        "driver" => "DriverOversightAction",
        "passenger" => "PassengerRestrictionAction",
        "membership" => "MembershipCheckAction",
        "quality" => "QualityFlagAction",
        "reports" => "ReportExportAction",
        _ => "AdminAction",
    };
    let p = json!({"actor":actor,"event":event,"detail":detail,"id":Uuid::now_v7(),"activity":activity});
    query(db,"WITH ev AS (INSERT INTO admin_events(id,actor_id,target_id,event_type,detail) VALUES (($1->>'id')::uuid,($1->>'actor')::uuid,$2,$1->>'event',$1->'detail') RETURNING *) INSERT INTO activity_logs(id,user_id,activity_type,target_id,target_type,details,created_at) SELECT id,actor_id,to_jsonb($1->>'activity')::text,target_id,'paid_rides',jsonb_build_object('event',event_type,'detail',detail),created_at FROM ev RETURNING details AS data",p,target).await?;
    Ok(())
}

/// Commit an admin mutation and its audit event atomically, then publish safety updates.
pub(crate) async fn mutate(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    event: &str,
    sql: &str,
    p: Value,
) -> Result<Value, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    let result = db
        .transaction::<Value, AdminError, _>(|db| {
            Box::pin(async move {
                let data = query(db, sql, p.clone(), id).await?;
                audit(db, c.user_id, id, event, p).await?;
                Ok(data)
            })
        })
        .await?;
    if event.starts_with("sos.") || event.starts_with("incident.") {
        let published = if event.starts_with("sos.") {
            "sos.updated"
        } else if event == "incident.opened" {
            event
        } else {
            "incident.updated"
        };
        s.ws_registry.broadcast_admin_event(
            "admin:safety",
            published,
            json!({"id":id,"action":event,"data":result}),
        );
    }
    Ok(result)
}

/// Validate a string against the allowed values for an API enum.
pub(crate) fn member(v: &str, set: &[&str]) -> Result<(), AdminError> {
    if set.contains(&v) {
        Ok(())
    } else {
        Err(bad("Invalid enum value"))
    }
}

/// Require a nonempty text value with a bounded payload size.
pub(crate) fn text(v: &str) -> Result<(), AdminError> {
    if v.trim().is_empty() || v.len() > 10000 {
        Err(bad("Text must be nonempty and at most 10000 bytes"))
    } else {
        Ok(())
    }
}
