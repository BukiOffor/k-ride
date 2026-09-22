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
    tracing::info!("Log emitted successfully");
    Ok(())
}