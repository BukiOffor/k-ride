//! Single-use, bounded-attempt password recovery with transactional email enqueue.
use crate::{AppState, Message, ModuleError};
use axum::{Json, extract::State};
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Uuid as SqlUuid},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgotPasswordInput {
    pub email: String,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPasswordInput {
    pub email: String,
    /// Six digits, including leading zeros.
    pub otp: String,
    pub new_password: String,
}
#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type = Jsonb)]
    data: Value,
}
async fn query(
    db: &mut diesel_async::AsyncPgConnection,
    sql: &str,
    p: Value,
    key: Uuid,
) -> Result<Value, ModuleError> {
    Ok(diesel::sql_query(sql)
        .bind::<Jsonb, _>(p)
        .bind::<SqlUuid, _>(key)
        .get_result::<Row>(db)
        .await?
        .data)
}
fn email(value: &str) -> Result<String, ModuleError> {
    let value = value.trim().to_lowercase();
    if value.len() > 254
        || value.chars().any(char::is_whitespace)
        || !value
            .split_once('@')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('@'))
    {
        return Err(ModuleError::BadRequest("A valid email is required"));
    }
    Ok(value)
}
fn mac(secret: &str, challenge: Uuid, otp: &str) -> Hmac<Sha256> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(b"password-reset-v1:");
    mac.update(challenge.as_bytes());
    mac.update(otp.as_bytes());
    mac
}
fn secret() -> Result<String, ModuleError> {
    std::env::var("JWT_SECRET").map_err(|_| {
        ModuleError::InternalError("Password recovery secret is not configured".into())
    })
}
fn valid_password(value: &str) -> bool {
    (8..=128).contains(&value.chars().count())
}
const INVALID: &str = "Invalid or expired reset code";

#[utoipa::path(post,path="/auth/forgot-password",tag="Auth",request_body=ForgotPasswordInput,responses((status=200,body=Message<String>),(status=400,description="Invalid email")))]
pub async fn forgot(
    State(s): State<Arc<AppState>>,
    Json(p): Json<ForgotPasswordInput>,
) -> Result<Json<Message<String>>, ModuleError> {
    let address = email(&p.email)?;
    let secret = secret()?;
    let otp = format!("{:06}", rand::rng().random_range(0..1_000_000u32));
    let challenge = Uuid::now_v7();
    let digest = hex::encode(mac(&secret, challenge, &otp).finalize().into_bytes());
    let mut db = s.pool.get().await?;
    db.transaction::<(),ModuleError,_>(|db| Box::pin(async move {
        // Lock the user so simultaneous requests cannot bypass cooldown or replace a code mid-reset.
        let user = diesel::sql_query("SELECT jsonb_build_object('id',id,'email',email) AS data FROM users WHERE lower(trim(email))=$1->>'email' AND is_active ORDER BY id LIMIT 1 FOR UPDATE")
            .bind::<Jsonb,_>(json!({"email":address})).get_result::<Row>(db).await.optional()?;
        let Some(user) = user else { return Ok(()); };
        let uid: Uuid = serde_json::from_value(user.data["id"].clone())?;
        let issued = query(db, r#"WITH issued AS (
 INSERT INTO password_reset_otps(user_id,challenge_id,otp_hash,expires_at)
 VALUES($2,($1->>'challenge')::uuid,$1->>'hash',now()+interval '10 minutes')
 ON CONFLICT(user_id) DO UPDATE SET challenge_id=EXCLUDED.challenge_id,otp_hash=EXCLUDED.otp_hash,
 requested_at=now(),expires_at=CASE WHEN password_reset_otps.expires_at<=now() OR password_reset_otps.consumed_at IS NOT NULL THEN EXCLUDED.expires_at ELSE password_reset_otps.expires_at END,
 attempts=CASE WHEN password_reset_otps.expires_at<=now() OR password_reset_otps.consumed_at IS NOT NULL THEN 0 ELSE password_reset_otps.attempts END,consumed_at=NULL
 WHERE password_reset_otps.requested_at<=now()-interval '60 seconds'
 AND (password_reset_otps.attempts<5 OR password_reset_otps.expires_at<=now() OR password_reset_otps.consumed_at IS NOT NULL)
 RETURNING expires_at)
 SELECT COALESCE((SELECT jsonb_build_object('expiresAt',expires_at) FROM issued),'null'::jsonb) AS data"#, json!({"challenge":challenge,"hash":digest}),uid).await?;
        if issued.is_null() { return Ok(()); }
        query(db, r#"INSERT INTO email_outbox(id,aggregate_id,event_key,recipient_id,recipient,subject,body)
 VALUES(($1->>'challenge')::uuid,($1->>'challenge')::uuid,'password-reset/'||($1->>'challenge'),$2,$1->>'email','K-Ride password reset code',$1->>'body') RETURNING '{}'::jsonb AS data"#,
            json!({"challenge":challenge,"email":user.data["email"],"body":format!("Your K-Ride password reset code is: {otp}\n\nThis code expires at {} (UTC). Only the most recently requested code works.\nIf you did not request this, you can ignore this email. Do not share this code.",issued["expiresAt"].as_str().unwrap_or("the end of the reset window"))}),uid).await?;
        Ok(())
    })).await?;
    let _ = s
        .mailer
        .try_send(crate::mailer::types::MailerEvent::DrainOutbox);
    Ok(Json(Message::context(
        "If an active account exists for this email, a reset code will be sent. Please wait 60 seconds before requesting another code.",
        "".into(),
    )))
}

#[utoipa::path(post,path="/auth/reset-password",tag="Auth",request_body=ResetPasswordInput,responses((status=200,body=Message<String>),(status=400,description="Invalid password or reset code")))]
pub async fn reset(
    State(s): State<Arc<AppState>>,
    Json(p): Json<ResetPasswordInput>,
) -> Result<Json<Message<String>>, ModuleError> {
    let address = email(&p.email)?;
    if !valid_password(&p.new_password) {
        return Err(ModuleError::BadRequest(
            "Password must contain 8 to 128 characters",
        ));
    }
    if p.otp.len() != 6 || !p.otp.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ModuleError::BadRequest(INVALID));
    }
    let secret = secret()?;
    // Hash outside the transaction and off the async executor, including unsuccessful requests.
    let hash =
        tokio::task::spawn_blocking(move || crate::helpers::password_hasher(&p.new_password))
            .await
            .map_err(|_| ModuleError::InternalError("Password hashing failed".into()))??;
    let mut db = s.pool.get().await?;
    let success = db.transaction::<bool,ModuleError,_>(|db|Box::pin(async move {
        let user = diesel::sql_query("SELECT jsonb_build_object('id',id) AS data FROM users WHERE lower(trim(email))=$1->>'email' AND is_active ORDER BY id LIMIT 1 FOR UPDATE")
            .bind::<Jsonb,_>(json!({"email":address})).get_result::<Row>(db).await.optional()?;
        let Some(user) = user else { return Ok(false); };
        let uid: Uuid = serde_json::from_value(user.data["id"].clone())?;
        let row = query(db, r#"SELECT COALESCE((SELECT jsonb_build_object('challenge',challenge_id,'hash',otp_hash) FROM password_reset_otps WHERE user_id=$2 AND expires_at>now() AND attempts<5 AND consumed_at IS NULL),'null'::jsonb) AS data WHERE $1 IS NOT NULL"#, json!({}),uid).await?;
        if row.is_null() { return Ok(false); }
        let challenge: Uuid = serde_json::from_value(row["challenge"].clone())?;
        let digest = hex::decode(row["hash"].as_str().unwrap_or("")).unwrap_or_default();
        let valid = mac(&secret, challenge, &p.otp).verify_slice(&digest).is_ok();
        // Return Ok(false), not Err: failed attempts must commit and survive retries.
        query(db,"UPDATE password_reset_otps SET attempts=attempts+1,consumed_at=CASE WHEN ($1->>'valid')::boolean THEN now() ELSE consumed_at END WHERE user_id=$2 RETURNING '{}'::jsonb AS data",json!({"valid":valid}),uid).await?;
        if !valid { return Ok(false); }
        query(db,"UPDATE users SET password_hash=$1->>'hash',updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"hash":hash}),uid).await?;
        Ok(true)
    })).await?;
    if !success {
        return Err(ModuleError::BadRequest(INVALID));
    }
    Ok(Json(Message::context(
        "Password reset successful. Please log in with your new password.",
        "".into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn otp_mac_is_bound_to_challenge_and_code() {
        let id = Uuid::now_v7();
        let digest = mac("secret", id, "001234").finalize().into_bytes();
        assert!(mac("secret", id, "001234").verify_slice(&digest).is_ok());
        assert!(mac("secret", id, "001235").verify_slice(&digest).is_err());
        assert!(
            mac("secret", Uuid::now_v7(), "001234")
                .verify_slice(&digest)
                .is_err()
        );
        assert!(
            mac("other secret", id, "001234")
                .verify_slice(&digest)
                .is_err()
        );
    }
    #[test]
    fn input_validation() {
        assert_eq!(email(" Test@Example.com ").unwrap(), "test@example.com");
        for value in ["", "bad", "a@@b", "a@", "a b@c"] {
            assert!(email(value).is_err());
        }
        assert!(!valid_password("short"));
        assert!(valid_password("12345678"));
        assert!(valid_password(&"x".repeat(128)));
        assert!(!valid_password(&"x".repeat(129)));
    }
}
