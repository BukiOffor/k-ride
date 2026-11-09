//! Administrative scheduled-booking discovery and audited manual assignment.
use super::*;
use crate::vrp::services::scheduled as bookings;

/// List every passenger's scheduled bookings, including history, with keyset pagination.
pub async fn list(
    s: Arc<AppState>,
    c: Claims,
    f: ScheduledRideFilters,
) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let params = f.params()?;
    let mut db = connection(&s).await?;
    query(&mut db, &page_sql("scheduled_ride_view", "($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'passengerId' IS NULL OR passenger_id=($1->>'passengerId')::uuid) AND ($1->>'driverId' IS NULL OR driver_id=($1->>'driverId')::uuid) AND ($1->>'from' IS NULL OR scheduled_at>=($1->>'from')::timestamptz AT TIME ZONE 'UTC') AND ($1->>'to' IS NULL OR scheduled_at<($1->>'to')::timestamptz AT TIME ZONE 'UTC')"), params, c.user_id).await
}

/// Assign an open booking through the same locks/checks as driver acceptance and audit atomically.
pub async fn assign(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: AssignScheduledDriver,
) -> Result<Value, AdminError> {
    require_admin(&c)?;
    if p.version < 1 {
        return Err(bad("version must be positive"));
    }
    let mut db = connection(&s).await?;
    let result = db
        .transaction::<Value, AdminError, _>(|db| {
            Box::pin(async {
                let (booking, changed) = bookings::reserve_locked(db, p.driver_id, id, p.version)
                    .await
                    .map_err(assignment_error)?;
                if changed {
                    audit(
                        db,
                        c.user_id,
                        id,
                        "scheduled.driver_assigned",
                        json!({"driverId":p.driver_id,"version":booking["version"]}),
                    )
                    .await?;
                }
                Ok(booking)
            })
        })
        .await?;
    // Database triggers queue the usual passenger/driver emails and in-app/realtime events.
    bookings::wake(&s);
    Ok(result)
}

/// Keep scheduling conflicts and authorization failures in the admin code/message envelope.
fn assignment_error(error: ModuleError) -> AdminError {
    match error {
        ModuleError::Conflict(message) => conflict(message),
        ModuleError::PermissionDenied => AdminError(
            StatusCode::FORBIDDEN,
            "driver_ineligible",
            "Driver is ineligible, is the passenger, or pickup has passed".into(),
        ),
        ModuleError::BadRequest(message) => bad(message),
        ModuleError::DieselError(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::CheckViolation
            | diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        )) => conflict("Conflicting booking or active journey"),
        ModuleError::DieselError(error) => error.into(),
        error => {
            tracing::error!(error=%error,"Admin scheduled assignment failed");
            AdminError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Scheduled assignment failed".into(),
            )
        }
    }
}
