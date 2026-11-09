//! Business logic and database operations for account.
use super::*;

/// Page through durable notifications belonging to the authenticated user.
pub async fn notifications(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    let mut db = connection(&s).await?;
    Ok(query(&mut db,&page_sql("(SELECT id,recipient_id,jsonb_build_object('id',id,'type',event_type,'payload',payload,'createdAt',created_at AT TIME ZONE 'UTC') AS data FROM ride_notifications) notifications","recipient_id=$2"),f.params()?,c.user_id).await?)
}

/// Read the authenticated user’s trusted contacts, including their phone numbers.
pub async fn contacts(s: Arc<AppState>, c: Claims) -> Result<Value, AdminError> {
    let mut db = connection(&s).await?;
    Ok(query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'name',name,'relationship',relationship,'phone',phone)),'[]'::jsonb) AS data FROM trusted_contacts WHERE user_id=$2 AND $1 IS NOT NULL",json!({}),c.user_id).await?)
}

/// Validate and save a trusted contact for the authenticated user.
pub async fn add_contact(s: Arc<AppState>, c: Claims, p: Contact) -> Result<Value, AdminError> {
    text(&p.name)?;
    text(&p.relationship)?;
    text(&p.phone)?;
    let mut db = connection(&s).await?;
    Ok(query(&mut db,"INSERT INTO trusted_contacts(id,user_id,name,relationship,phone) VALUES(($1->>'id')::uuid,$2,$1->>'name',$1->>'relationship',$1->>'phone') RETURNING jsonb_build_object('id',id,'name',name,'relationship',relationship,'phone',phone) AS data",json!({"id":Uuid::now_v7(),"name":p.name,"relationship":p.relationship,"phone":p.phone}),c.user_id).await?)
}

/// Delete a trusted contact only when it belongs to the authenticated user.
pub async fn remove_contact(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    let mut db = connection(&s).await?;
    Ok(query(&mut db,"DELETE FROM trusted_contacts WHERE id=($1->>'id')::uuid AND user_id=$2 RETURNING jsonb_build_object('id',id) AS data",json!({"id":id}),c.user_id).await?)
}
