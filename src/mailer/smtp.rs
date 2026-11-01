//! Independent asynchronous SMTP fallback; no synchronous socket calls run on Tokio worker threads.
use super::worker::EmailJob;
use lettre::message::{Mailbox, MultiPart};
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
    /// Submit HTML and plain-text alternatives using a stable Message-ID; remote SMTP servers may still deliver a retry twice.
    pub async fn send(&self, job: &EmailJob) -> Result<(), String> {
        let email = Self::message(&self.from, job)?;
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
    fn message(from: &Mailbox, job: &EmailJob) -> Result<Message, String> {
        let to: Mailbox = job
            .recipient
            .parse()
            .map_err(|_| "invalid_recipient".to_owned())?;
        let sender = from.email.to_string();
        let domain = sender.split('@').nth(1).unwrap_or("localhost");
        let html = super::templates::render(&job.subject, &job.body)
            .map_err(|_| "email_template_failed".to_owned())?;
        Message::builder()
            .from(from.clone())
            .to(to)
            .subject(&job.subject)
            .message_id(Some(format!("<{}@{}>", job.id, domain)))
            .multipart(MultiPart::alternative_plain_html(job.body.clone(), html))
            .map_err(|_| "invalid_message".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smtp_contains_both_mime_alternatives_and_stable_message_id() {
        let job = EmailJob {
            id: uuid::Uuid::now_v7(),
            recipient: "test@example.invalid".into(),
            subject: "K-Ride password reset code".into(),
            body: "Your K-Ride password reset code is: 001234\n\nDo not share this code.".into(),
            transport: "smtp".into(),
            attempts: 1,
        };
        let email = Smtp::message(&"K-Ride <mail@example.invalid>".parse().unwrap(), &job).unwrap();
        let raw = String::from_utf8(email.formatted()).unwrap();
        assert!(raw.contains("multipart/alternative"));
        assert!(raw.contains("text/plain"));
        assert!(raw.contains("text/html"));
        assert!(raw.contains("001234"));
        assert!(raw.contains(&format!("<{}@example.invalid>", job.id)));
    }
}
