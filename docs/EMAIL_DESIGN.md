# Transactional email design

All active outbox emails share the K-Ride HTML layout in
`src/mailer/templates/transactional.html`, rendered by `src/mailer/templates.rs`.
The design uses a forest-green header, lime accent, warm neutral background,
responsive content card, and system fonts. It does not depend on remote images,
web fonts, JavaScript, or external stylesheets. Important styles are inline;
mobile spacing is enhanced with a media query. Tables and an Outlook container
fallback keep the layout usable in email clients with limited CSS support.

## Email variants

- Password reset: security heading, prominent six-digit code (including leading
  zeros), the original expiry instructions, and a security footer. The code is
  excluded from the preheader.
- Scheduled ride updates: greeting and event message, followed by pickup,
  destination, pickup time/timezone, locked fare, and booking reference. This
  covers confirmation, assignment, reminders, cancellation, expiry, and linked
  journey updates using their existing event text.
- Other outbox subjects: the same branded shell with all original text lines.

The renderer escapes subject and body values, including customer names and
locations. Arbitrary HTML in those values is displayed as text, not executed.

Resend receives both `html` and the unchanged `text` body. SMTP sends a
`multipart/alternative` message with plain text first and HTML second. Existing
outbox records are rendered at delivery time, so no database migration or client
API change is required. Already delivered messages do not change. Legacy channel
events that have no sending handler remain inactive; this change styles messages
that actually pass through the outbox.

## Local previews

Generate previews with fictional data without sending an email:

```sh
cargo run --example preview_emails -- /tmp/kride-email-previews
```

Open `password-reset.html`, `scheduled-ride.html`, and `general.html` from that
directory in a browser. The example calls the production renderer. To change the
palette, spacing, and typography, edit the bundled HTML template and rebuild the
server. Copy and email-specific grouping live in `templates.rs`.

Check previews at desktop and mobile widths. Gmail, Outlook, Apple Mail, dark mode,
and other clients may vary in font rendering, corner rounding, and color handling;
a live inbox check remains useful before rollout. Both transports retain the
plain-text alternative for clients that cannot display HTML.
