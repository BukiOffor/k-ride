//! Durable leased email delivery with a bounded channel wakeup, primary API, and SMTP fallback.
use super::{
    resend::{Resend, Submission},
    smtp::Smtp,
    types::MailerEvent,
};
use crate::vrp::services::scheduled::{connection, query};
use crate::{AppState, ModuleError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailJob {
    pub id: Uuid,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub transport: String,
    pub attempts: i32,
}
/// Injectable fallback adapter keeps real SMTP and test transports on the same async delivery path.
#[async_trait::async_trait]
pub(crate) trait FallbackSender: Send + Sync {
    /// Submit one immutable outbox email without retaining a database connection.
    async fn submit(&self, job: &EmailJob) -> Result<(), String>;
}
#[async_trait::async_trait]
impl FallbackSender for Smtp {
    /// Adapt the reusable SMTP transport to the worker's injectable fallback interface.
    async fn submit(&self, job: &EmailJob) -> Result<(), String> {
        self.send(job).await
    }
}
pub(crate) struct Delivery {
    pub primary: Option<Resend>,
    pub fallback: Option<Arc<dyn FallbackSender>>,
}

/// Start four bounded workers sharing reusable async clients; periodic polling recovers dropped channel wakeups.
pub fn start(
    s: Arc<AppState>,
    receiver: async_channel::Receiver<MailerEvent>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let delivery = Arc::new(Delivery {
        primary: Resend::from_env(),
        fallback: Smtp::from_env().map(|smtp| Arc::new(smtp) as Arc<dyn FallbackSender>),
    });
    if delivery.primary.is_none() {
        tracing::warn!("Resend not configured; email jobs will use SMTP fallback");
    }
    if delivery.fallback.is_none() {
        tracing::warn!(
            "SMTP fallback not configured; failed email jobs remain visible in the outbox"
        );
    }
    (0..4).map(|_|{let s=s.clone();let rx=receiver.clone();let transport=delivery.clone();tokio::spawn(async move{
  let mut timer=tokio::time::interval(std::time::Duration::from_secs(5));
  loop{
   tokio::select!{_ = timer.tick()=>{},event=rx.recv()=>{match event{Ok(MailerEvent::DrainOutbox)=>{},Ok(other)=>{tracing::warn!(event=%other.name(),"Legacy mail event requires a template handler");continue;},Err(_)=>break}}}
   for _ in 0..20{match deliver_one(&s,&transport).await{Ok(true)=>{},Ok(false)=>break,Err(e)=>{tracing::error!(error=%e,"Email outbox worker failed");break;}}}
  }
 })}).collect()
}
/// Lease one due email; ordering per booking/recipient prevents newer updates overtaking failed earlier updates.
pub(crate) async fn claim(s: &AppState, token: Uuid) -> Result<Option<EmailJob>, ModuleError> {
    let mut db = connection(s).await?;
    let value=query(&mut db,r#"WITH candidate AS (
 SELECT e.id FROM email_outbox e WHERE e.sent_at IS NULL AND e.dead_at IS NULL AND e.next_attempt_at<=(now() AT TIME ZONE 'UTC') AND (e.lease_until IS NULL OR e.lease_until<(now() AT TIME ZONE 'UTC'))
 AND NOT EXISTS(SELECT 1 FROM email_outbox prior WHERE prior.aggregate_id=e.aggregate_id AND prior.recipient_id=e.recipient_id AND prior.sent_at IS NULL AND prior.dead_at IS NULL AND prior.sequence_id<e.sequence_id)
 ORDER BY e.next_attempt_at,e.sequence_id LIMIT 1 FOR UPDATE SKIP LOCKED), leased AS (
 UPDATE email_outbox SET lease_token=$2,lease_until=(now() AT TIME ZONE 'UTC')+interval '90 seconds',attempts=attempts+1 WHERE id IN(SELECT id FROM candidate) RETURNING *)
 SELECT COALESCE((SELECT to_jsonb(leased) FROM leased),'null'::jsonb) AS data WHERE $1 IS NOT NULL"#,json!({}),token).await?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(serde_json::from_value(value)?))
    }
}
/// Attempt one leased job without a database connection or transaction held during network I/O.
pub(crate) async fn deliver_one(s: &AppState, delivery: &Delivery) -> Result<bool, ModuleError> {
    let token = Uuid::now_v7();
    let Some(mut job) = claim(s, token).await? else {
        return Ok(false);
    };
    if job.transport == "resend" {
        let outcome = if let Some(primary) = &delivery.primary {
            primary.send(&job).await
        } else {
            Submission::Rejected
        };
        match outcome {
            Submission::Accepted(provider_id) => {
                finish(s, &job, token, Some(provider_id), None).await?;
                return Ok(true);
            }
            Submission::Uncertain if job.attempts < 3 => {
                finish(s, &job, token, None, Some("resend_submission_uncertain")).await?;
                return Ok(true);
            }
            _ => {
                // Persist transport selection before SMTP I/O so a crash does not switch back to the primary.
                let mut db = connection(s).await?;
                query(&mut db,"UPDATE email_outbox SET transport='smtp',last_error='primary_failed_or_uncertain' WHERE id=($1->>'job')::uuid AND lease_token=$2 RETURNING '{}'::jsonb AS data",json!({"job":job.id}),token).await?;
                job.transport = "smtp".into();
            }
        }
    }
    let failure = match &delivery.fallback {
        Some(smtp) => smtp.submit(&job).await.err(),
        None => Some("smtp_not_configured".into()),
    };
    finish(
        s,
        &job,
        token,
        if failure.is_none() {
            Some(format!("smtp:{}", job.id))
        } else {
            None
        },
        failure.as_deref(),
    )
    .await?;
    Ok(true)
}
/// Persist submission success or a bounded retry/dead-letter result using the current lease token.
async fn finish(
    s: &AppState,
    job: &EmailJob,
    token: Uuid,
    provider_id: Option<String>,
    error: Option<&str>,
) -> Result<(), ModuleError> {
    let mut db = connection(s).await?;
    let delay = retry_seconds(job.attempts);
    query(&mut db,"UPDATE email_outbox SET sent_at=CASE WHEN $1->>'provider' IS NOT NULL THEN (now() AT TIME ZONE 'UTC') ELSE NULL END,provider_message_id=$1->>'provider',last_error=$1->>'error',dead_at=CASE WHEN $1->>'provider' IS NULL AND attempts>=8 THEN (now() AT TIME ZONE 'UTC') ELSE NULL END,next_attempt_at=(now() AT TIME ZONE 'UTC')+make_interval(secs=>($1->>'delay')::int),lease_token=NULL,lease_until=NULL WHERE id=($1->>'job')::uuid AND lease_token=$2 RETURNING '{}'::jsonb AS data",json!({"job":job.id,"provider":provider_id,"error":error,"delay":delay}),token).await?;
    if error.is_some() {
        tracing::warn!(job=%job.id,attempt=job.attempts,reason=?error,"Email submission failed or uncertain");
    }
    Ok(())
}
/// Exponential retry delays stay within the primary provider's 24-hour idempotency window.
pub fn retry_seconds(attempt: i32) -> i32 {
    (30_i32.saturating_mul(2_i32.saturating_pow(attempt.saturating_sub(1).clamp(0, 10) as u32)))
        .min(900)
}
