use crate::schema::activity_logs;
use super::*;
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