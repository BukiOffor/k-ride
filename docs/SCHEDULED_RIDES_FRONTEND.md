# Scheduled rides: frontend integration

This guide describes the implemented scheduled-ride API for passenger and driver screens.
See [backend policy and deployment](SCHEDULED_RIDES.md) for worker and email configuration.

## Connection conventions

- REST base: `/api/v1`. Prefix every REST path below with it.
- Use existing authentication: `Authorization: Bearer <access_token>` or the existing
  HTTP-only access-token cookie. Browser requests using cookies need `credentials: "include"`.
- Send `Content-Type: application/json` for request bodies.
- JSON uses camelCase. Successful responses are direct objects, without a `data` wrapper.
- Request bodies and list queries reject unknown fields.
- IDs are UUID strings. Dates are ISO 8601 strings; submit an explicit UTC offset or `Z`.

## Endpoint map

| Method | Path | Caller | Success response |
| --- | --- | --- | --- |
| POST | `/rides/scheduled` | Passenger | 201, booking |
| GET | `/rides/scheduled` | Passenger | 200, page of own bookings |
| GET | `/rides/scheduled/{id}` | Authorized participant or eligible browsing driver | 200, booking |
| PATCH | `/rides/scheduled/{id}` | Passenger who booked | 200, booking |
| POST | `/rides/scheduled/{id}/cancel` | Passenger who booked | 200, booking |
| GET | `/driver/scheduled-requests` | Driver | 200, page |
| POST | `/driver/scheduled-requests/{id}/accept` | Eligible driver | 200, booking |
| POST | `/driver/scheduled-requests/{id}/cancel` | Assigned driver | 200, booking |

A user with the Driver role may also book as a passenger. The passenger list shows
bookings they created, not bookings they accepted as a driver.

## Shared response types

```ts
export type ScheduledStatus =
  | "OPEN" | "RESERVED" | "ACTIVATED"
  | "COMPLETED" | "CANCELLED" | "EXPIRED";

export interface Money {
  amountMinor: number;
  currency: string;
}

export interface Place {
  id?: string;
  name: string;
  secondary?: string;
  point: { lat: number; lng: number };
  source?: string;
}

export interface FareQuote {
  total: Money;
  range: { low: Money; high: Money };
  breakdown: Array<{ code: string; label: string; amount: Money }>;
  isEstimate: boolean;
  waiting: { freeMinutes: number; perMinute: Money };
}

export interface ScheduledRide {
  id: string;
  passengerId: string;
  driverId: string | null;
  rideId: string | null;
  status: ScheduledStatus;
  version: number;
  pickup: Place;
  destination: Place;
  scheduledAt: string;
  timezone: string;
  durationSeconds: number;
  bufferMinutes: number;
  total: Money;
  fare: FareQuote;
  fareLocked: boolean;
  option: { id: string; name: string | null; vehicleClass: string | null };
  cancellationReason: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ScheduledRidePage {
  items: ScheduledRide[];
  nextCursor: string | null;
  total: number;
}
```

`amountMinor` uses currency minor units: NGN amounts are in kobo. Display the locked
`total`; do not recalculate or submit a fare. The saved fare has `isEstimate: false`,
a range equal to its total, and zero extra waiting charges. The booking does not
include driver contact details, vehicle details, or route geometry.

## Passenger: create a booking

First call the existing `POST /rides/estimate` with `pickup` and `destination` places.
Use its `estimateId` and the selected `options[].option.id` to create the booking.
Obtain a fresh estimate when the passenger is ready to confirm so the quote has not expired.
An option with `available: false` can still be scheduled; shared options cannot.

`POST /api/v1/rides/scheduled`

```json
{
  "estimateId": "019a0abc-1234-7000-8000-000000000001",
  "optionId": "go",
  "scheduledAt": "2026-10-23T10:00:00+01:00",
  "timezone": "Africa/Lagos"
}
```

The date above is illustrative: select a pickup between **one hour and thirty days
from submission**. `timezone` must be an IANA timezone recognized by the server.
Resolve the selected local date/time in that timezone before submitting; the device
may use a different timezone. The server stores the instant in UTC.

Do not include pickup, destination, fare, passengerId, driverId, or paymentMethodId
in this request. Journey and fare come from the server estimate.

The 201 response is a `ScheduledRide`, initially `OPEN`. Matching is queued
immediately, with the worker normally broadcasting within its five-second scan.
Show “Finding a driver” until the booking becomes `RESERVED`.

The passenger/estimate pair identifies one booking. Retrying the same estimate,
option, instant and timezone returns that booking, also with 201. This makes a
lost-response retry safe. Reusing that estimate with different booking parameters
returns 409; obtain a new estimate for a separate booking.

## Passenger: list and detail

```text
GET /api/v1/rides/scheduled?limit=25
GET /api/v1/rides/scheduled?status=RESERVED&limit=25
GET /api/v1/rides/scheduled/{id}
```

The list includes history unless filtered. Detail is accessible to the passenger,
assigned driver, Admin/Root, or an eligible driver browsing an open future request.
Unauthorized access may return 404. A browsing driver can lose access when another
driver accepts the booking.

### List queries and pagination

Both list endpoints use these parameters:

| Parameter | Meaning |
| --- | --- |
| `cursor` | UUID from the previous response's `nextCursor`; omit for the first request |
| `limit` | Integer 1–100; default 25 |
| `status` | One exact uppercase status; omit for all statuses allowed by the list |
| `from` | Inclusive pickup timestamp |
| `to` | Exclusive pickup timestamp |
| `mine` | Driver endpoint: `true` for assigned bookings; default `false` for the open board |

Responses are `ScheduledRidePage`. `total` is the filtered count before applying
the cursor; `nextCursor: null` ends pagination. Results are ordered by descending
UUID, **not pickup time**. Sort fetched items for a chronological presentation and
use date filters when appropriate. Reset the cursor when filters change.

Use `URLSearchParams` to encode timestamps, especially offsets containing `+`.
There are no `page`, `offset`, search, or multiple-status query parameters.
`mine` has no effect on the passenger endpoint.

## Passenger: change pickup time

`PATCH /api/v1/rides/scheduled/{id}`

```json
{
  "scheduledAt": "2026-10-24T10:00:00+01:00",
  "timezone": "Africa/Lagos"
}
```

Only the passenger can reschedule an `OPEN` or `RESERVED` booking. The same
one-hour-to-thirty-day window applies. The response becomes `OPEN`: any assigned
driver is released, the request is broadcast again, and a driver must accept the
new time. Fare, places, option, estimated duration and saved buffer stay unchanged.

Tell the passenger that changing the time requires fresh driver acceptance.
Changing the route or option requires cancelling and booking from a fresh estimate.

## Passenger: cancel

`POST /api/v1/rides/scheduled/{id}/cancel`

```json
{
  "reason": "My travel plans changed"
}
```

`reason` is required, nonempty after trimming, and at most 2,000 UTF-8 bytes.
The passenger can cancel `OPEN` or `RESERVED` bookings; the response is `CANCELLED`.
Closed bookings return 409. For `ACTIVATED`, use the linked live ride's existing
passenger cancellation endpoint: `POST /api/v1/rides/{rideId}/cancel`.

## Driver: available requests and own reservations

```text
GET /api/v1/driver/scheduled-requests?limit=25
GET /api/v1/driver/scheduled-requests?mine=true&status=RESERVED&limit=25
```

The default board contains future `OPEN` requests the driver is eligible to serve,
including requests that received no response to the initial broadcast. The driver
must be active, approved on the Professional/Both track, and have an eligible
verified vehicle with the required class and capacity. Being online is not required.
Their own passenger requests are excluded.

`mine=true` returns bookings currently assigned to that driver, including history
unless filtered. To show reservations, use `mine=true&status=RESERVED`.
Filtering the default open board by `status=RESERVED` does not list reservations.
Released bookings have no assigned driver and disappear from the driver's own list.

The board does not prefilter every possible scheduling overlap. Acceptance performs
the final checks, so handle 409 even when a card was visible.

## Driver: accept a request

`POST /api/v1/driver/scheduled-requests/{id}/accept`

```json
{
  "version": 1
}
```

Send the **version from the card the driver reviewed**. On success, replace the card
with the returned `RESERVED` booking. Acceptance does not mark the driver as on-trip;
the booking has no live `rideId` until activation.

The server locks acceptance and checks booking version, eligibility, pickup deadline,
passenger and driver commitments, and current live trips. Two competing drivers
cannot reserve the same booking. Self-acceptance is forbidden.

On 409, refresh the card/list and explain that the request changed, was accepted,
or conflicts with another journey. **Do not automatically accept again with a newer
version**: the pickup time may have changed. The driver must review it again.
A retry after a lost response returns the existing reservation when it is already
reserved by that same driver.

## Driver: release a reservation

`POST /api/v1/driver/scheduled-requests/{id}/cancel`

```json
{
  "reason": "I can no longer make this pickup"
}
```

Only the assigned driver can release it. The reason has the same validation as
passenger cancellation. Before pickup, the response becomes `OPEN`, clears `driverId`,
retains the fare, and queues another broadcast. If pickup has passed, it expires.

For `ACTIVATED`, use `POST /api/v1/driver/rides/{rideId}/cancel`. A pre-start driver
cancellation reopens the scheduled booking when pickup is still in the future.
Use the existing live-ride flow for arrival, start and completion as well.

## Status and timing for screens

| Status | Suggested display | Frontend behavior |
| --- | --- | --- |
| `OPEN` | Finding a driver | Passenger can reschedule/cancel; eligible drivers can accept |
| `RESERVED` | Driver confirmed | Passenger can reschedule/cancel; assigned driver can release |
| `ACTIVATED` | Ride ready | Open the existing live-ride screen using `rideId` |
| `COMPLETED` | Completed | Show history |
| `CANCELLED` | Cancelled | Show history |
| `EXPIRED` | No ride available | Offer a fresh estimate and new booking |

- Unmatched requests broadcast again at 30, 15 and 5 minutes before pickup, and
  expire at pickup. A booking is not guaranteed until a driver accepts.
- Reserved bookings get a reminder within one hour of pickup. Activation begins
  15 minutes before pickup and creates a normal ride with the assigned driver.
- Busy participants can defer activation; an unactivated reservation expires after
  ten minutes past pickup. An ineligible driver can cause reopening before pickup.
- Navigate based on the server's `status` and `rideId`, never just a local countdown.
- The saved buffer defaults to 30 minutes **after estimated journey end**. A reserved
  interval covers `scheduledAt` through `scheduledAt + durationSeconds + bufferMinutes`.
  Read `bufferMinutes` from the response; deployment settings can change future bookings.
- Display pickup using the saved `timezone`, for example with `Intl.DateTimeFormat`.

## WebSocket updates

Connect to **`/rides/ws` at the server root**, using `wss:` in production. This path
has no `/api/v1` prefix. Browser WebSockets use the existing authentication cookie;
browser WebSocket constructors cannot set an Authorization header. Do not put tokens
in the URL. Non-browser clients can use the existing Bearer authentication mechanism.

Passengers and assigned drivers can subscribe to a booking:

```json
{"type":"subscribe","channel":"scheduled:019a0abc-1234-7000-8000-000000000002"}
```

```json
{"type":"unsubscribe","channel":"scheduled:019a0abc-1234-7000-8000-000000000002"}
```

Browsing drivers receive broadcast hints through their authenticated driver mailbox;
there is no `driver:scheduled` channel to subscribe to. A driver who has not accepted
a request cannot subscribe to its private booking channel. Released drivers lose access.
Acceptance always uses HTTP, not a WebSocket command.

Send an application heartbeat every 30 seconds:

```json
{"type":"ping"}
```

The server responds with `{"type":"pong"}`. Its incoming idle timeout is 45 seconds;
server protocol pings do not replace this application heartbeat.

Events use this envelope; `booking` has the full `ScheduledRide` shape above:

```ts
interface ScheduledEventFrame {
  type: "event";
  event: {
    id: string;
    seq: number;
    rideId?: string;
    rideVersion?: number;
    occurredAt: string;
    payload: {
      type: string;
      scheduledRideId?: string;
      booking?: ScheduledRide;
    };
  };
}
```

Relevant payload types:

- `scheduled.opened`, `scheduled.reopened`, `scheduled.matching_reminder`
- `scheduled.reserved`, `scheduled.reminder`, `scheduled.activated`
- `scheduled.completed`, `scheduled.cancelled`, `scheduled.expired`
- `scheduled.ride_*` hints for linked live-ride changes
- `scheduled.resync_required`: refetch REST; this hint can omit the booking and ID

The booking snapshot reflects current state at delivery; the event name describes
the triggering change. Do not derive status solely from the event name. Deduplicate
by event ID, use booking versions to avoid replacing newer state, and refetch on
reconnect, focus, action completion or conflict. Scheduled events use `seq: 0`, so
sequence numbers cannot identify duplicates or missed scheduled updates.

When activated, also subscribe to the existing `ride:{rideId}` channel for live-ride
and location updates. Keep the booking subscription to detect cancellation/reopening.

The WebSocket registry is process-local. Poll visible lists/detail approximately
every 10–15 seconds as a fallback, particularly with multiple backend instances.
Clear timers and unsubscribe on screen cleanup; refresh after returning to the screen.
Durable in-app notifications use the existing `GET /api/v1/rides/me/notifications`
endpoint; scheduled notifications carry `scheduledRideId` in their payload.

## Errors and safe retries

| HTTP status | Frontend handling |
| --- | --- |
| 400 / 422 | Validate payload, timezone, time window, required fields or query values |
| 401 | Use the existing session refresh/sign-in flow |
| 403 | Show the supplied restriction or driver eligibility message |
| 404 | Booking is missing or no longer accessible; refresh/remove the card |
| 409 | Refresh and let the user review the current booking before another action |
| 415 | Send JSON with the correct Content-Type |
| 500 / network failure | Show retry state; do not assume a mutation failed to commit |

A scheduling conflict has this shape:

```json
{
  "code": "scheduling_conflict",
  "message": "Conflicting booking or active journey"
}
```

Other business errors can use `{ "message": "...", "status_code": 400 }`.
Request restrictions can use `code: "requests_restricted"`; missing records can use
`code: "not_found"`. Framework validation errors may be plain text. Parse responses
defensively instead of assuming every error is JSON. Existing missing-authentication
handling may also return 400; reuse the application's auth handling.

For an uncertain create result, retry the identical estimate and payload. For an
uncertain acceptance, refetch the booking and verify the assigned driver; never
silently accept changed booking details. After an uncertain cancel/reschedule, refetch
before deciding whether to offer another action.

## Email and confirmation UI

The server queues emails to the passenger and assigned/previous driver as appropriate
for creation, assignment, changes, reminders, cancellation, expiry and completion.
Resend is the primary provider; SMTP is the fallback. Frontend requests never call
either provider or contain provider credentials.

Successful booking HTTP responses confirm the database operation, not email delivery.
There is no frontend email-delivery status endpoint. Show booking state from REST and
in-app notifications; do not block confirmation screens while waiting for an email.
