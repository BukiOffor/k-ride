//! Primary transactional email adapter. API submission is asynchronous and bounded by a timeout.
use super::worker::EmailJob;

#[derive(Clone)]
pub struct Resend {
    client: reqwest::Client,
    key: String,
    from: String,
    url: String,
    next_request: std::sync::Arc<tokio::sync::Mutex<tokio::time::Instant>>,
}
#[derive(Debug, PartialEq)]
pub enum Submission {
    Accepted(String),
    Rejected,
    Uncertain,
}
impl Resend {
    /// Build the provider once at startup; credentials stay out of job payloads and logs.
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("RESEND_API_KEY")
            .ok()
            .filter(|x| !x.trim().is_empty())?;
        let from = std::env::var("EMAIL_FROM")
            .ok()
            .filter(|x| !x.trim().is_empty())?;
        Some(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .ok()?,
            key,
            from,
            url: "https://api.resend.com/emails".into(),
            next_request: std::sync::Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
        })
    }
    /// Submit an immutable outbox email using the job UUID as Resend's idempotency key.
    pub async fn send(&self, job: &EmailJob) -> Submission {
        // Pace submissions to two starts/second across these workers without retaining a DB connection.
        {
            let mut next = self.next_request.lock().await;
            tokio::time::sleep_until(*next).await;
            *next = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
        }
        let response=self.client.post(&self.url).bearer_auth(&self.key).header("Idempotency-Key",format!("scheduled-email/{}",job.id))
   .json(&serde_json::json!({"from":self.from,"to":[job.recipient],"subject":job.subject,"text":job.body})).send().await;
        match response {
            Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
                Ok(v) => v["id"]
                    .as_str()
                    .map(|s| Submission::Accepted(s.into()))
                    .unwrap_or(Submission::Uncertain),
                Err(_) => Submission::Uncertain,
            },
            Ok(r)
                if r.status().is_client_error()
                    && r.status() != reqwest::StatusCode::TOO_MANY_REQUESTS
                    && r.status() != reqwest::StatusCode::CONFLICT =>
            {
                Submission::Rejected
            }
            _ => Submission::Uncertain,
        }
    }
    #[cfg(test)]
    /// Construct a local test transport without reading real credentials or contacting the real API.
    pub(crate) fn test_endpoint(url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(200))
                .build()
                .unwrap(),
            key: "test".into(),
            from: "test@example.invalid".into(),
            url,
            next_request: std::sync::Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
        }
    }
}
