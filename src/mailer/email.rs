use handlebars::Handlebars;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use serde_json::Value;

use super::config::Config;

/// An Email struct that represents a receiver, a sender and email configuration.
pub struct Email {
    to: Receiptent,
    from: String,
    config: Config,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Receiptent {
    pub name: String,
    pub email: String,
}

impl Email {
    pub fn new(to: Receiptent, config: Config) -> Self {
        let from = format!("Admin <{}>", config.smtp_from.to_owned());
        Email { to, from, config }
    }

    fn new_transport(
        &self,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
        let creds = Credentials::new(
            self.config.smtp_user.to_owned(),
            self.config.smtp_pass.to_owned(),
        );
        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(
            &self.config.smtp_host.to_owned(),
        )?
        .port(self.config.smtp_port)
        .credentials(creds)
        .build();
        Ok(transport)
    }

    fn render_template(
        &self,
        template_name: &str,
        data: Value,
    ) -> Result<String, handlebars::RenderError> {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_file(template_name, &format!("./templates/{}.hbs", template_name))?;
        let content_template = handlebars.render(template_name, &data)?;
        tracing::info!("Rendered Html succesfully ... ");
        Ok(content_template)
    }

    pub async fn send_email(
        &self,
        template_name: &str,
        subject: &str,
        data: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let html_template = self.render_template(template_name, data)?;
        let email = Message::builder()
            .to(
                format!("{} <{}>", self.to.name.as_str(), self.to.email.as_str())
                    .parse()
                    .unwrap(),
            )
            .reply_to(self.from.as_str().parse().unwrap())
            .from(self.from.as_str().parse().unwrap())
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_template)?;

        let transport = self.new_transport()?;
        transport.send(email).await?;
        tracing::info!("Email sent succesfully ... ");
        Ok(())
    }
}
