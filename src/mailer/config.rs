use crate::errors::ModuleError;

#[derive(Debug, Clone)]
pub struct Config {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub smtp_from: String,
    pub smtp_to: String,
}

impl Config {
    pub fn init() -> Result<Config, ModuleError> {
        let smtp_host = std::env::var("SMTP_HOST")?;
        let smtp_port = std::env::var("SMTP_PORT").unwrap_or("2525".into());
        let smtp_user = std::env::var("SMTP_USER")?;
        let smtp_pass = std::env::var("SMTP_PASS")?;
        let smtp_from = std::env::var("SMTP_FROM")?;
        let smtp_to = std::env::var("SMTP_TO")?;

        Ok(Config {
            smtp_host,
            smtp_pass,
            smtp_user,
            smtp_port: smtp_port.parse::<u16>().unwrap(),
            smtp_from,
            smtp_to,
        })
    }
}
