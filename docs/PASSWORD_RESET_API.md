# Forgot password and OTP reset API

## Flow

1. Ask the user for their account email.
2. Call `POST /api/v1/auth/forgot-password`.
3. Display a six-digit OTP input and a new-password input.
4. Submit email, OTP, and new password to `POST /api/v1/auth/reset-password`.
5. On success, clear any locally stored session and return the user to login.

Both endpoints are public: no bearer token or authentication cookie is required. Use your existing API origin. Send `Content-Type: application/json`. Responses use the existing `{ "message": "...", "data": "" }` envelope.

There is no separate OTP verification endpoint. Verification and password replacement happen together, so a code cannot be verified and then reused to perform a second reset.

## Request a code

```http
POST /api/v1/auth/forgot-password
Content-Type: application/json
```

```json
{
  "email": "customer@example.com"
}
```

Success: `200 OK`.

```json
{
  "message": "If an active account exists for this email, a reset code will be sent. Please wait 60 seconds before requesting another code.",
  "data": ""
}
```

The response is identical for an unknown email, inactive account, cooldown, and exhausted-attempt window. Do not interpret success as confirmation that an account exists or an email was delivered.

The email subject is `K-Ride password reset code`. The body contains the six-digit code and its UTC expiry time. Delivery runs asynchronously through the existing email outbox, using Resend with SMTP fallback.

## Reset the password

```http
POST /api/v1/auth/reset-password
Content-Type: application/json
```

```json
{
  "email": "customer@example.com",
  "otp": "001234",
  "newPassword": "MyNewPassword123!"
}
```

| Field | Required | Rules |
| --- | --- | --- |
| `email` | Yes | Account email; trimmed and matched without case sensitivity |
| `otp` | Yes | String containing exactly six ASCII digits; retain leading zeros |
| `newPassword` | Yes | String containing 8–128 characters |

Unknown fields are rejected on both endpoints. All inputs must be strings. Keep password confirmation in the client; do not send a `confirmPassword` field. The server does not trim the password.

Success: `200 OK`.

```json
{
  "message": "Password reset successful. Please log in with your new password.",
  "data": ""
}
```

The code is consumed atomically with the password update. It cannot be reused. Success does not log the user in, issue new tokens, or send another email. The new password is stored using the existing salted Argon2 password hasher.

## Expiry, attempts, and resend behaviour

- Initial codes expire after 10 minutes, measured from issuance, not email delivery.
- At most one code can be issued per account every 60 seconds. Requests during cooldown still return the generic success response.
- Resend uses the same forgot-password endpoint. A newly issued code immediately invalidates the previous code.
- Resending within an active reset window preserves the original expiry time and failed-attempt count. It does not grant another full 10 minutes.
- Five incorrect, correctly formatted OTP submissions exhaust the active reset window. The fifth returns the same invalid-code response; even the correct code will then be rejected.
- After exhaustion, wait until the window expires and request another code. Repeated resend requests do not bypass the attempt limit.
- After expiry or successful consumption, a new request can start a fresh window, subject to the 60-second issuance cooldown.
- Malformed OTPs and invalid passwords are rejected before verification and do not consume a verification attempt.

Show “Use the most recent code” and disable resend for 60 seconds after each request. Because delivery is asynchronous, older emails can arrive after a resend; only the newest issued code works.

## Errors

Business validation errors return JSON:

```json
{
  "message": "Bad request: Invalid or expired reset code",
  "status_code": 400
}
```

The invalid-code message covers a wrong, expired, consumed, or exhausted code, an unknown email, and an inactive account. Do not try to distinguish these cases from the response.

| HTTP status | Meaning | Client action |
| --- | --- | --- |
| `400` | Invalid email, invalid password length, or invalid/expired reset code | Show the message; allow correction or a new-code request |
| `415` | Missing or unsupported JSON content type | Send `Content-Type: application/json` |
| `422` | Missing fields, wrong types, or unknown fields | Correct the request body |
| `500` | Database, hashing, or configuration failure | Show a temporary error and retry option |

Other validation messages are `Bad request: A valid email is required` and `Bad request: Password must contain 8 to 128 characters`. Malformed JSON may return `400`. Framework extraction errors may be plain text, so parse error bodies defensively.

A reset retry after an uncertain network response can return “Invalid or expired reset code” if the first reset already succeeded. Let the user try signing in with the new password or request a fresh code.

## Browser integration example

```ts
const API_BASE = "https://<your-api-host>/api/v1";

async function postAuth(path: string, body: object) {
  const response = await fetch(`${API_BASE}/auth/${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let payload: any;
  try { payload = JSON.parse(text); }
  catch { payload = { message: text }; }
  if (!response.ok) {
    throw new Error(payload.message || `Request failed (${response.status})`);
  }
  return payload as { message: string; data: string };
}

export function requestPasswordReset(email: string) {
  return postAuth("forgot-password", { email });
}

export function resetPassword(email: string, otp: string, newPassword: string) {
  return postAuth("reset-password", { email, otp, newPassword });
}
```

Use a text input with `inputmode="numeric"` and `autocomplete="one-time-code"` for OTPs; never convert them to numbers. Use `autocomplete="new-password"` for the new password. Keep OTPs and passwords out of URLs, analytics, console output, and persistent client storage.

## Deployment and current session behaviour

Apply `migrations/2026-11-01-010000_password_reset` with `diesel migration run` before using the endpoints. The existing email-outbox migration must already be applied.

`JWT_SECRET` is required and also protects the stored OTP digest. Configure the existing mail worker with `RESEND_API_KEY` and `EMAIL_FROM`, and/or SMTP fallback variables (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM`). Start the server normally; mail workers already run at startup. OTP plaintext is present in the protected email outbox for delivery; the reset challenge table stores only a keyed digest.

This feature does not revoke existing access or refresh JWTs. Previously issued tokens retain their existing expiry behaviour. Clear the current client's local session on reset success; server-side revocation of other sessions is outside this implementation. Deploy these public endpoints behind the service's normal request-rate limits in addition to the per-account OTP limits.

The endpoints and schemas are included in `/swagger-ui` and `/api-docs/openapi.json`.
