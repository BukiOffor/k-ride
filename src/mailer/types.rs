use serde::{Deserialize, Serialize};

/// An Event over a channel that triggers mail actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailerEvent {
    SendInvitationEmail {
        name: String,
        url: String,
        to: String,
    },
    SendWelcomeEmail {
        name: String,
        to: String,
    },
    SendPasswordResetEmail {
        name: String,
        url: String,
        to: String,
    },
    SendPasswordChangeEmail {
        name: String,
        to: String,
    },
    SendAccountDeletionEmail {
        name: String,
        to: String,
    },
    SendAccountDeactivationEmail {
        name: String,
        to: String,
    },
    SendAccountActivationEmail {
        name: String,
        to: String,
    },
    SendSessionStartEmail {
        name: String,
        to: String,
    },
    SendSessionAddedEmail {
        name: String,
        to: String,
    },
    SendOtp {
        name: String,
        to: String,
        otp: String,
    },
}

impl MailerEvent {
    pub fn name(&self) -> String {
        match self {
            MailerEvent::SendInvitationEmail { .. } => "SendInvitationEmail".to_string(),
            MailerEvent::SendWelcomeEmail { .. } => "SendWelcomeEmail".to_string(),
            MailerEvent::SendPasswordResetEmail { .. } => "SendPasswordResetEmail".to_string(),
            MailerEvent::SendPasswordChangeEmail { .. } => "SendPasswordChangeEmail".to_string(),
            MailerEvent::SendAccountDeletionEmail { .. } => "SendAccountDeletionEmail".to_string(),
            MailerEvent::SendAccountDeactivationEmail { .. } => {
                "SendAccountDeactivationEmail".to_string()
            }
            MailerEvent::SendAccountActivationEmail { .. } => {
                "SendAccountActivationEmail".to_string()
            }
            MailerEvent::SendSessionStartEmail { .. } => "SendSessionStartEmail".to_string(),
            MailerEvent::SendSessionAddedEmail { .. } => "SendSessionAddedEmail".to_string(),
            Self::SendOtp { .. } => "SendOtp".to_string(),
        }
    }
}
