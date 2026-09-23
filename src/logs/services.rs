use super::dto::GetActivityLogsQuery;
use super::*;
use crate::schema::activity_logs;
use diesel_async::RunQueryDsl;

pub async fn emit_log(
    payload: ActivityLog,
    conn: &mut crate::Connection<'_>,
) -> Result<(), ModuleError> {
    diesel::insert_into(activity_logs::table)
        .values(&payload)
        .execute(&mut **conn)
        .await?;
    tracing::debug!("Log emitted successfully");
    Ok(())
}

pub async fn emit_logs(
    payloads: &[ActivityLog],
    conn: &mut crate::Connection<'_>,
) -> Result<(), ModuleError> {
    if payloads.is_empty() {
        return Ok(());
    }
    diesel::insert_into(activity_logs::table)
        .values(payloads)
        .execute(&mut **conn)
        .await?;
    tracing::debug!("Batch of {} logs emitted successfully", payloads.len());
    Ok(())
}

pub async fn get_logs(
    pool: Arc<Pool>,
    params: GetActivityLogsQuery,
) -> Result<Vec<ActivityLog>, ModuleError> {
    use crate::schema::activity_logs::dsl::*;

    let mut conn = pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::activity_logs::table.into_boxed::<diesel::pg::Pg>();

    if let Some(uid) = params.user_id {
        query = query.filter(user_id.eq(uid));
    }

    if let Some(at) = params.activity_type {
        query = query.filter(activity_type.eq(at));
    }

    if let Some(tid) = params.target_id {
        query = query.filter(target_id.eq(Some(tid)));
    }

    if let Some(ref tt) = params.target_type {
        query = query.filter(target_type.eq(Some(tt.clone())));
    }

    query = query.order(created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let results = query
        .load::<ActivityLog>(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    Ok(results)
}

pub async fn get_log_by_id(pool: Arc<Pool>, log_id: Uuid) -> Result<ActivityLog, ModuleError> {
    use crate::schema::activity_logs::dsl::*;

    let mut conn = pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let log = activity_logs
        .filter(id.eq(log_id))
        .first::<ActivityLog>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ModuleError::NotFoundError("Log not found".into()),
            _ => ModuleError::InternalError(e.to_string().into()),
        })?;

    Ok(log)
}
