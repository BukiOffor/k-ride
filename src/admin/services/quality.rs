//! Business logic and database operations for quality.
use super::*;

/// List open or cleared quality flags with rating aggregates.
pub async fn quality(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',f.id,'subject',p.data-'email'-'homeArea'-'inspection'-'deactivationReason','averageRating',p.data->'rating','lowRatings',p.data->'lowRatings','rides',p.data->'completedRides','reason',f.reason,'source',f.source,'flaggedAt',f.flagged_at AT TIME ZONE 'UTC') ORDER BY f.flagged_at DESC),'[]'::jsonb) AS data FROM admin_quality_flags f JOIN admin_people_view p ON p.id=f.subject_id WHERE ($1->>'role' IS NULL OR p.role=$1->>'role') AND (CASE WHEN COALESCE($1->>'status','open')='open' THEN f.cleared_at IS NULL ELSE f.cleared_at IS NOT NULL END) AND $2 IS NOT NULL",f.params()?,c.user_id).await?)
}

/// Clear a quality flag and retain its history and optional reviewer note.
pub async fn clear_flag(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: Note,
) -> Result<Value, AdminError> {
    Ok(mutate(&s,&c,id,"quality.cleared","UPDATE admin_quality_flags SET cleared_at=(now() AT TIME ZONE 'UTC'),clear_note=$1->>'note' WHERE id=$2 AND cleared_at IS NULL RETURNING jsonb_build_object('id',id,'clearedAt',cleared_at AT TIME ZONE 'UTC') AS data",json!({"note":p.note})).await?)
}
