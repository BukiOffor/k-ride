//! Generate local HTML previews with fictional data; never sends email.
//! cargo run --example preview_emails -- /tmp/kride-email-previews
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("kride-email-previews"));
    std::fs::create_dir_all(&directory)?;
    let samples = [
        (
            "password-reset.html",
            "K-Ride password reset code",
            "Your K-Ride password reset code is: 084291\n\nThis code expires at 2026-12-01T10:10:00+00:00 (UTC). Only the most recently requested code works.\nIf you did not request this, you can ignore this email. Do not share this code.",
        ),
        (
            "scheduled-ride.html",
            "K-Ride scheduled ride update",
            "Hello Ada,\n\nA driver has accepted your scheduled ride.\nPickup: Nnamdi Azikiwe International Airport\nDestination: Transcorp Hilton, Abuja\nPickup time: 01 Dec 2026, 11:00 (Africa/Lagos)\nLocked fare: NGN 40000.00.\nBooking: 019a3e00-0000-7000-8000-000000000001\n\nCheck the app for the current status.",
        ),
        (
            "general.html",
            "A little update from K-Ride",
            "Hello Ada,\n\nHere is the latest update on your account.\nThank you for choosing K-Ride.",
        ),
    ];
    for (filename, subject, body) in samples {
        let path = directory.join(filename);
        std::fs::write(&path, k_rides::mailer::templates::render(subject, body)?)?;
        println!("{}", path.display());
    }
    Ok(())
}
