# Scheduled rides

Frontend contracts and integration examples: [Scheduled rides frontend guide](SCHEDULED_RIDES_FRONTEND.md).

The backend immediately broadcasts a new scheduled request. Drivers accept from
that broadcast or from the scheduled-request board using the same locked endpoint.
No match is guaranteed until a driver accepts. The board contains all eligible open
requests, including those that did not get a response to their initial broadcast.

## Booking and fare policy

- Use the existing `/rides/estimate` endpoint, then schedule with `estimateId`,
  `optionId`, `scheduledAt` (an ISO timestamp with an explicit offset), and `timezone`
  (an IANA name such as `Africa/Lagos`). Expired estimates and shared options are rejected.
- Pickup must be between one hour and thirty days ahead. Times are stored in UTC;
  email confirmations display the saved timezone. Ambiguous local times must be
  resolved by the client into an explicit offset before submission.
- The server saves the complete quote, route and option snapshot at booking. Client
  amounts are never accepted. The final fare stays locked, including zero extra
  waiting charges; a later pricing update does not change it.
- Each passenger/estimate pair identifies one booking; retries return that booking
  when its time, timezone and option match. Get a fresh estimate for a separate booking.
- The default buffer is **30 minutes after estimated journey end**, configurable
  with `SCHEDULED_RIDE_BUFFER_MINUTES` (0–240). It is saved per booking so changes
  to the setting do not change existing commitments. An adjacent journey can begin
  exactly at the end of that buffer. The same overlap policy also guards immediate rides.

## API

All paths are under `/api/v1`; use the existing access JWT or cookie.

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/rides/scheduled` | Book from a valid server estimate; returns 201 |
| GET | `/rides/scheduled` | Passenger's bookings |
| GET | `/rides/scheduled/{id}` | Passenger, assigned driver, admin, or eligible driver browsing an open request |
| PATCH | `/rides/scheduled/{id}` | `{ scheduledAt, timezone }`; retain journey/fare and reopen for fresh driver consent |
| POST | `/rides/scheduled/{id}/cancel` | Passenger cancellation with `{ reason }` |
| GET | `/driver/scheduled-requests` | Eligible open requests; `mine=true` lists the driver's reservations |
| POST | `/driver/scheduled-requests/{id}/accept` | `{ version }` from the reviewed card; locked acceptance |
| POST | `/driver/scheduled-requests/{id}/cancel` | Assigned driver releases a reservation with `{ reason }` |

Lists use `cursor`, `limit` (default 25, max 100), `status`, `from`, `to`; responses
are `{ items, nextCursor, total }`. They are ordered by descending UUID. Detail
contains no phones, email addresses or document bodies. The driver must be active,
approved on Professional/Both track, and have a verified vehicle of the required
class and capacity. Offline drivers can browse and reserve future requests.

## Acceptance, cancellation and activation

1. Creation commits an `OPEN` booking plus email and broadcast jobs. No network call
   is made under the booking transaction. A worker announces queued requests within
   its five-second scan interval.
2. Acceptance locks passenger/driver user rows in UUID order, then the booking row.
   It rechecks eligibility, deadline, passenger overlaps, driver overlaps and current
   live trips before setting `RESERVED`. A competing, stale-version or conflicting acceptance returns
   409. Driver availability is unchanged until activation.
3. A driver releasing a reservation changes it back to `OPEN`, retaining its fare,
   and broadcasts it again. The previous driver and passenger get state notifications.
4. Unmatched requests rebroadcast at **30, 15 and 5 minutes** before pickup. Missed
   thresholds after downtime collapse into one fresh announcement. Open requests
   expire at pickup time rather than silently remaining available forever.
5. A reserved booking sends a reminder within one hour of pickup. At fifteen minutes
   before pickup, a worker links it to exactly one ordinary `DRIVER_ASSIGNED` ride.
   Busy participants defer activation; after ten minutes past pickup, the unactivated
   reservation expires. An ineligible assigned driver causes reopening while pickup
   is still in the future.
6. After activation, use the existing live-ride arrival/start/completion/cancellation
   endpoints. The scheduled cancellation endpoint returns 409 with that instruction.
   A pre-start driver cancellation of the linked ride reopens and rebroadcasts the
   booking if pickup is still ahead. Passenger cancellation closes it. Started trips
   cannot be reopened as scheduled requests.
7. Completion synchronizes the booking and queues completion messages to both parties.

The live-ride overlap trigger takes the same user locks as reservation acceptance,
protecting against races between immediate and scheduled commitments. Estimated
trip duration is a planning constraint, not a guarantee against traffic delays;
activation still rechecks actual live-trip occupancy. Booking writes also enqueue
durable in-app notifications through the existing notification endpoint.

## Realtime

Drivers receive `scheduled.opened`, `scheduled.reopened`,
`scheduled.matching_reminder`, `scheduled.reserved` and lifecycle hints through their
existing authenticated WebSocket mailbox. The board remains the source of truth;
refetch it on reconnect or a hint. There is no automatic assignment from a broadcast.

Passengers and assigned drivers may subscribe to `scheduled:{bookingId}` on the
existing authenticated socket. Authorization is checked at subscription and again
before each forwarded event, so a released driver loses access. Admin/Root can
inspect booking channels. `scheduled.resync_required` means refetch REST.
Event payloads include `type`, `scheduledRideId` and a current `booking` summary.
Hints may be delivered more than once after a worker crash; use event IDs/booking
versions and refetch rather than treating hints as commands.

The existing WebSocket registry is process-local. With several application instances,
a claimed broadcast reaches connections on that instance; REST polling/reconnect
refresh remains required until a shared WebSocket pub/sub fan-out is deployed.
Database locking, booking activation, email leases and the board are multi-instance safe.

## Email channel and fallback

The sender in `AppState.mailer` is a bounded `async_channel::Sender<MailerEvent>`.
Handlers use non-waiting wakeups after commit. Email contents live in `email_outbox`,
not solely in RAM, so a full channel or restart does not lose messages.

Four background workers claim due jobs with `FOR UPDATE SKIP LOCKED` and a 90-second
lease token, release the connection, then submit with reusable async clients. Jobs
for the same booking/recipient are ordered. Messages cover booking creation,
assignment, reopening, reminders, expiry, cancellation and material live-ride changes;
the assigned/previous driver receives updates as appropriate.

The primary transport uses [Resend's send-email API](https://resend.com/docs/api-reference/emails/send-email).
Each job has a stable idempotency key; Resend retains those keys for
[24 hours](https://resend.com/docs/dashboard/emails/idempotency-keys). An explicit primary
rejection or missing primary configuration selects SMTP immediately. Uncertain API
failures retry the same Resend key twice, then select SMTP on the third attempt.
Once selected, SMTP stays selected across restarts. It uses a stable Message-ID,
STARTTLS on port 587 or implicit TLS on port 465, and bounded async timeouts.

**Cross-provider delivery is at-least-once:** an ambiguous API failure followed by
SMTP can produce a duplicate if the API had already accepted the message. SMTP
receivers are not required to deduplicate Message-ID. `sent_at` means the transport
accepted submission, not that the message reached an inbox. No bounce/delivery
webhook is installed. Failed jobs retry with exponential backoff (30 seconds up to
15 minutes), then become dead letters after eight attempts; inspect `last_error`,
`dead_at`, `transport` and `provider_message_id` operationally. Logs omit credentials,
recipient addresses and email bodies.

Copy settings from `scheduled-rides.env.example` into your deployment configuration.
Use a verified sender domain and configure its authentication for each transport.
The application starts without credentials, but email cannot be delivered until at
least one transport is configured. Existing template-oriented mail classes are retained;
the new worker handles scheduled outbox messages, not legacy template events.

## Files and validation

- DTOs/models: `src/vrp/dtos/scheduled.rs`, `src/vrp/models/scheduled.rs`.
- HTTP/service layers: `src/vrp/handlers/scheduled.rs`, `src/vrp/services/scheduled.rs`.
- Maintenance: `src/vrp/scheduled_worker.rs`; tables/triggers in the scheduled migration.
- Mail transports/workers: `src/mailer/resend.rs`, `smtp.rs`, `worker.rs`.
- Swagger includes the new endpoints and booking schemas.

Run normal checks with `cargo test --offline --lib` and `cargo check --offline`.
Prepare all production scheduled/email SQL against a migrated database with
`python3 tests/validate_scheduled_sql.py postgresql://...`.
The ignored integration test needs a **dedicated disposable**, migrated database
and Redis instance; it commits uniquely named test fixtures and uses a local fake
email API/injected SMTP sender, never real mail credentials:

```sh
SCHEDULED_TEST_DATABASE_URL=postgresql://... \
SCHEDULED_TEST_REDIS_URL=redis://127.0.0.1:55441/ \
cargo test --offline --lib vrp::scheduled_tests::scheduled_workflow_and_email_failover -- --ignored
```
