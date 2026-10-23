//! Independent asynchronous SMTP fallback; no synchronous socket calls run on Tokio worker threads.
use super::worker::EmailJob;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct Smtp {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}
impl Smtp {
    /// Build a reusable authenticated TLS transport from SMTP_HOST/PORT/USER/PASS/FROM.
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("SMTP_HOST").ok()?;
        let user = std::env::var("SMTP_USER").ok()?;
        let pass = std::env::var("SMTP_PASS").ok()?;
        let port = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".into())
            .parse::<u16>()
            .ok()?;
        let from = std::env::var("SMTP_FROM")
            .or_else(|_| std::env::var("EMAIL_FROM"))
            .ok()?
            .parse()
            .ok()?;
        let builder = if port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&host).ok()?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host).ok()?
        };
        Some(Self {
            transport: builder
                .port(port)
                .credentials(Credentials::new(user, pass))
                .timeout(Some(std::time::Duration::from_secs(10)))
                .build(),
            from,
        })
    }
    /// Submit plain-text email using a stable Message-ID; remote SMTP servers may still deliver a retry twice.
    pub async fn send(&self, job: &EmailJob) -> Result<(), String> {
        let to: Mailbox = job
            .recipient
            .parse()
            .map_err(|_| "invalid_recipient".to_owned())?;
        let sender = self.from.email.to_string();
        let domain = sender.split('@').nth(1).unwrap_or("localhost");
        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(&job.subject)
            .message_id(Some(format!("<{}@{}>", job.id, domain)))
            .body(job.body.clone())
            .map_err(|_| "invalid_message".to_owned())?;
        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            self.transport.send(email),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err("smtp_submission_failed".into()),
            Err(_) => Err("smtp_submission_timeout".into()),
        }
    }
}
