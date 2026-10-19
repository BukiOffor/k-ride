# Paid Rides administration API

All REST routes below are under `/api/v1`. The backend lives in this repository;
frontend service adapters and removal of the preview label belong in the dashboard repository.

## Authentication and conventions

- `/admin/*` requires a valid access JWT with `Admin` or `Root` role. Send
  `Authorization: Bearer <token>` or the existing `access_token` HTTP-only cookie.
  No inspector role is introduced.
- JSON names are camelCase. Lists return `{ items, nextCursor, total }`.
  `cursor` is the previous page's `nextCursor` UUID. Results use descending UUID
  keyset order, with `limit=25` by default and a maximum of 100. Totals count the
  filtered set before the cursor. Counts for chips describe the workspace before
  the individual status filter.
- Admin errors are `{ code, message }`; malformed JSON/query/path arguments and
  authentication errors retain the application's existing Axum/JWT rejection shapes.
- Money uses integer `amountMinor` and `currency`. Missing ratings, prior-period
  comparisons with a zero denominator, and undefined rates are null, never fabricated.
- Driver tracks remain `Volunteer`, `Professional`, `Both`; applications use the
  explicit uppercase status set below. Ride statuses retain the existing paid-rides
  enum. The frontend should reuse its paid-rides status adapter.
- Admin list endpoints exclude document bodies. Fetch documents only when opening
  an application review. Driver/passenger phones appear on ride/incident details
  only while an open or under-review incident covers them. People rosters and
  trusted-contact summaries do not release phone numbers.
- Admin changes and their activity log entries commit together. `/logs` exposes
  dedicated safety, incident, verification, inspection, oversight, restriction, membership, quality and export activity types, with the specific event in `details.event`. `admin_events` also
  keeps incident, verification and ride status timelines.

## Network rides and metrics

| Method | Path | Filters / notes |
| --- | --- | --- |
| GET | `/admin/overview` | `period=week\|month\|quarter`; current primary counts, period performance metrics and latest active SOS |
| GET | `/admin/rides` | `state=active\|all`, `status`, `sos`, `q`, `cursor`, `limit`; chip counts included |
| GET | `/admin/rides/{id}` | Network detail, identifiers, reference, route, fare/payment, timestamps, tracking, safety and durable events |
| GET | `/admin/reports` | `period=week\|month\|quarter` (7/30/90 trailing days), previous-period deltas and daily chart series |
| GET | `/admin/reports/export` | Same period; synchronous CSV download, not email |

The current domain has one passenger per ride. Admin summaries expose that passenger
in a `passengers` array. This API does not implement pooled matching, extra passengers,
or per-passenger routing. Historical charts use stored records; availability and
pending-review snapshots from before this migration cannot be reconstructed as true
historical sparklines and are omitted.

## Safety and incidents

| Method | Path | Body / notes |
| --- | --- | --- |
| GET | `/admin/sos` | `status=active` returns sent/acknowledged/responding alerts |
| POST | `/admin/sos/{id}/acknowledge` | No body |
| POST | `/admin/sos/{id}/resolve` | `{ "note": "optional" }` |
| GET | `/admin/incidents` | `status`, `severity`, `category`, `q`, `cursor`, `limit`; chip counts included |
| GET | `/admin/incidents/{id}` | Participants, timeline and notes; phones absent on resolved records |
| POST | `/admin/incidents` | `{ rideId?, category, severity, title, description, participantIds: [] }` |
| PATCH | `/admin/incidents/{id}` | `{ status?, severity?, assigneeId? }`; OPEN/UNDER_REVIEW/RESOLVED; reopening supported |
| POST | `/admin/incidents/{id}/notes` | `{ "note": "..." }` |
| POST | `/admin/incidents/{id}/calls` | `{ "participantId": "uuid" }`; logs call initiation and returns phone/callHref only for an open incident participant |

`POST /rides/{id}/sos` and `/safety-reports` require ride participation and return
HTTP 202. Their inserts atomically create incidents through a database trigger.
SOS opens `SOS_ACTIVATION` / `CRITICAL`; reports use `MEDIUM` and their supplied
category. Existing safety events are backfilled at migration time.

Categories: SAFETY_CONCERN, SOS_ACTIVATION, DRIVER_CONDUCT, PASSENGER_CONDUCT,
VEHICLE_ISSUE, PAYMENT_DISPUTE, OTHER. Severity: LOW, MEDIUM, HIGH, CRITICAL.

Resolving an incident closes its linked safety event, updates the ride's SOS state,
and stores `incident.resolved` notifications for its participants. Resolving an SOS
alone does not close the incident: the safety desk may still be reviewing it.
New SOS alerts also create durable notifications for active Admin/Root accounts.
Call logging records initiation, not confirmation of an actual telephone connection.

## Driver verification

Application states: DRAFT, SUBMITTED, DOCUMENT_REVIEW, INSPECTION_REQUIRED,
INSPECTION_SCHEDULED, FINAL_REVIEW, APPROVED, CHANGES_REQUIRED, REJECTED.
Legacy pending/approved strings are normalized at the database boundary.

| Method | Path | Body / notes |
| --- | --- | --- |
| GET | `/admin/driver-applications` | `status`, `track`, `q`, `cursor`, `limit`; no Base64 bodies |
| GET | `/admin/driver-applications/{userId}` | Documents, licence, affiliation, inspection, comments, requested changes |
| PATCH | `/admin/driver-applications/{userId}/documents/{type}` | `{ status: VERIFIED\|NEEDS_ATTENTION, note? }`; nonempty reason required when flagging |
| POST | `/admin/driver-applications/{userId}/comments` | `{ body, visibility: INTERNAL\|APPLICANT, documentType? }` |
| POST | `/admin/driver-applications/{userId}/inspection` | `{ scheduledAt, location, note? }`; future ISO date-time required |
| POST | `/admin/driver-applications/{userId}/decision` | `{ decision: APPROVE\|REQUEST_CHANGES\|REJECT, reason?, changesRequested?: [] }` |
| GET | `/rides/me/driver-review` | Applicant's own feedback, documents, inspection and deactivation reason; no internal comments |

Required documents: GOVERNMENT_ID, PROFILE_PHOTO, DRIVERS_LICENCE,
VEHICLE_REGISTRATION, INSURANCE, ROADWORTHINESS_CERTIFICATE, VEHICLE_PHOTO;
GUARANTOR_PHOTO is also required for non-workers. Approval returns 409 until all
required documents are verified and the licence is current. Approval verifies the
linked vehicle in the same transaction. Replacing document contents invalidates
that document's review. Licence expiry is checked at read/approval time.
Request-changes/reject decisions require a reason; request-changes also requires
`changesRequested`. Reasons are published as applicant-visible comments.
Internal discussion stays internal. Drivers must have an approved application and
verified vehicle to go online.

## People and quality

| Method | Path | Body / notes |
| --- | --- | --- |
| GET | `/admin/drivers` | `q` (name/email/plate), `track`, `availability`, `verification`, `flagged`, `active`, pagination |
| GET | `/admin/drivers/{id}` | Identity, vehicle, inspection, affiliation, rating, trip and weekly earning totals |
| GET | `/admin/drivers/{id}/rides` | That driver's own history, pagination |
| POST / DELETE | `/admin/drivers/{id}/flag` | POST `{ reason }`; DELETE clears open manual flag |
| POST | `/admin/drivers/{id}/deactivate` | `{ reason }`; deactivates account, sets offline, withdraws offers and removes spatial index entry |
| POST | `/admin/drivers/{id}/inspection` | Same appointment body as application inspection |
| GET | `/admin/passengers` | `q`, `membership`, `hasTrustedContact`, pagination |
| GET | `/admin/passengers/{id}` | Identity/stats and trusted-contact names/relationships |
| GET | `/admin/passengers/{id}/rides` | That passenger's history, pagination |
| POST / DELETE | `/admin/passengers/{id}/restriction` | POST `{ reason }`; blocks new `/rides/request` with 403 `requests_restricted`; ongoing trips continue |
| POST | `/admin/passengers/{id}/membership-check` | Records CHECKING; no external membership provider is configured |
| GET | `/admin/quality-flags` | `role=Driver\|User`, `status=open` (or `closed`) |
| POST | `/admin/quality-flags/{id}/clear` | `{ note? }`; retains ratings |

Use existing `PATCH /users/{id}/activate` to reactivate an account. Quality flags
are manual; no automatic rating threshold is assumed. Chat/conversation delivery,
track-switch requests and automatic inspection completion are not domain workflows
in this repository; absent values are not synthesized.

Own-user support routes:

- `GET /rides/me/notifications?cursor=&limit=`: durable in-app notifications.
- `GET /rides/me/trusted-contacts`: the caller's full contact list.
- `POST /rides/me/trusted-contacts`: `{ name, relationship, phone }`.
- `DELETE /rides/me/trusted-contacts/{id}`: own contact only.

## WebSocket

Connect to `/rides/ws` with the access-token cookie or a Bearer token for non-browser
clients. Legacy `user_id`/`driver_id` hints must match the JWT; they cannot impersonate
another user. `/rides/ws/{driverId}` requires that driver's authenticated identity.
This is an intentional change from the previous unauthenticated socket.

Send `{ "type": "subscribe", "channel": "admin:rides" }` or `admin:safety`.
Only Admin/Root may subscribe to those channels. Ride participants and Admin/Root
may subscribe to `ride:{uuid}`. Invalid/unauthorized subscriptions receive no data.
Admin channels use the existing event wrapper, with unique event IDs; event names are in `event.payload.type`:

- `admin.ride.changed`: compact ride ID/status/SOS/version hints; refetch REST.
- `sos.raised`, `sos.updated`, `incident.opened`, `incident.updated`: refetch active
  SOS or the affected incident. `payload.action` identifies the specific mutation.
- `admin.resync_required`: a slow subscriber missed events; refetch both lists.

The same event-ID and event-type semantics apply on existing ride channels. Location updates go only to the
ride channel, never the network channel. Send `{ "type": "ping" }` at least every
30 seconds; the socket has a 45-second incoming-idle timeout.

Socket registries are process-local, as in the existing dispatcher. Multi-instance
broadcast fan-out is not implemented. REST/in-app notifications are durable; clients
should refetch on reconnect and poll safety/live lists every 10–15 seconds as a
fallback. Browser Web Push and email delivery are intentionally outside this agreed
implementation; WebSocket and in-app notifications are the delivery channels.

## Validation

`cargo test --offline --lib` runs unit tests. A migrated disposable PostgreSQL database
is needed for the ignored integration test:

```sh
ADMIN_TEST_DATABASE_URL=postgresql://... cargo test --offline --lib \
  admin::tests::database_projection_and_safety_contracts -- --ignored
python3 tests/validate_admin_sql.py postgresql://...
```

The integration test rolls back its fixtures. It covers legacy statuses, document
approval blocking/replacement invalidation, vehicle verification, incident creation,
phone release, resolution notifications, reports, paging and ride restrictions.
`tests/validate_admin_sql.py` only prepares statements; it does not modify data.
