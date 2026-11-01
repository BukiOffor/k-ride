//! One HTML presentation shared by both transports. Stored plain text remains the
//! source of truth, including for jobs queued before HTML delivery was introduced.
use handlebars::Handlebars;
use serde::Serialize;
use std::sync::LazyLock;

static TEMPLATES: LazyLock<Handlebars<'static>> = LazyLock::new(|| {
    let mut templates = Handlebars::new();
    templates
        .register_template_string(
            "transactional",
            include_str!("templates/transactional.html"),
        )
        .expect("bundled email template must compile");
    templates
});
#[derive(Serialize)]
struct Detail<'a> {
    label: &'a str,
    value: &'a str,
}
#[derive(Serialize)]
struct Content<'a> {
    subject: &'a str,
    preheader: &'a str,
    category: &'a str,
    headline: &'a str,
    subtitle: &'a str,
    paragraphs: Vec<&'a str>,
    otp: Option<&'a str>,
    details: Vec<Detail<'a>>,
    after: Vec<&'a str>,
    footer_note: &'a str,
}

/// Escape every variable through Handlebars; customer names and place labels
/// must never become HTML. Unrecognized bodies receive the same branded shell.
pub fn render(subject: &str, body: &str) -> Result<String, handlebars::RenderError> {
    let mut content = Content {
        subject,
        preheader: subject,
        category: "A NOTE FROM K-RIDE",
        headline: subject,
        subtitle: "Everything you need to know, in one place.",
        paragraphs: body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect(),
        otp: None,
        details: vec![],
        after: vec![],
        footer_note: "Keep this message for your records.",
    };
    if subject == "K-Ride password reset code" {
        // Only transform the known generated first line. Never extract an OTP
        // from arbitrary prose or include it in a subject/preheader.
        if let Some((first, rest)) = body.split_once('\n') {
            if let Some(code) = first.strip_prefix("Your K-Ride password reset code is: ") {
                if code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()) {
                    content.category = "ACCOUNT SECURITY";
                    content.headline = "A fresh start. A secure account.";
                    content.subtitle = "Let's get you back on the move.";
                    content.preheader = "Use your one-time code to reset your K-Ride password.";
                    content.paragraphs = vec![
                        "We received a request to reset your password. Enter the code below in the app, along with your new password.",
                    ];
                    content.otp = Some(code);
                    content.after = rest
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .collect();
                    content.footer_note =
                        "K-Ride will never ask you to share your password or reset code.";
                }
            }
        }
    } else if subject == "K-Ride scheduled ride update" {
        content.category = "YOUR SCHEDULED RIDE";
        content.headline = "Your journey, kept in view.";
        content.subtitle = "Your booking details, always close at hand.";
        content.footer_note = "Open the K-Ride app to view your booking and its current status.";
        let mut paragraphs = vec![];
        let mut after = vec![];
        for line in body.lines().filter(|line| !line.trim().is_empty()) {
            let detail = [
                "Pickup",
                "Destination",
                "Pickup time",
                "Locked fare",
                "Booking",
            ]
            .iter()
            .find_map(|label| {
                line.strip_prefix(&format!("{label}: "))
                    .map(|value| Detail { label, value })
            });
            if let Some(detail) = detail {
                content.details.push(detail);
            } else if line == "Check the app for the current status." {
                after.push(line);
            } else {
                paragraphs.push(line);
            }
        }
        content.paragraphs = paragraphs;
        content.after = after;
    }
    TEMPLATES.render("transactional", &content)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn otp_preserves_leading_zeros_and_expiry_without_leaking_into_preheader() {
        let html = render("K-Ride password reset code", "Your K-Ride password reset code is: 001234\n\nThis code expires at 2026-12-01T10:10:00+00:00 (UTC).\nIf you did not request this, ignore it.").unwrap();
        assert!(html.contains(">001234</p>"));
        assert!(html.contains("2026-12-01T10:10:00+00:00"));
        assert!(html.contains("If you did not request this"));
        assert_eq!(html.matches("001234").count(), 1);
    }
    #[test]
    fn scheduled_details_and_untrusted_content_are_escaped() {
        let html = render("K-Ride scheduled ride update", "Hello <img src=x onerror=alert(1)>,\n\nYour driver has arrived.\nPickup: A & B\nDestination: <script>bad()</script>\nPickup time: 01 Dec 2026, 10:00 (Africa/Lagos)\nLocked fare: NGN 40000.00.\nBooking: abc-123\n\nCheck the app for the current status.").unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        for value in [
            "&lt;img",
            "A &amp; B",
            "&lt;script&gt;",
            "NGN 40000.00.",
            "abc-123",
            "Your driver has arrived.",
        ] {
            assert!(html.contains(value));
        }
    }
    #[test]
    fn fallback_keeps_all_lines_and_escapes_subject() {
        let html = render("Welcome <test>", "First line\n\nSecond line <unsafe>").unwrap();
        assert!(html.contains("Welcome &lt;test&gt;"));
        assert!(html.contains("First line"));
        assert!(html.contains("Second line &lt;unsafe&gt;"));
        // Malformed OTP input must remain visible rather than silently losing text.
        let html = render(
            "K-Ride password reset code",
            "Unexpected message\nPlease contact support.",
        )
        .unwrap();
        assert!(html.contains("Unexpected message"));
        assert!(html.contains("Please contact support."));
    }
}
