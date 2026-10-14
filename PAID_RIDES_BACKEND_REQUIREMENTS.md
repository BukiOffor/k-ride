# Paid Rides — Backend Requirements

What the Paid Rides frontend expects from the backend. It is technology-agnostic: any stack that serves these HTTP endpoints and a WebSocket (or an equivalent realtime channel behind the same contract) will work. The TypeScript shapes the frontend uses are in `src/features/paid-rides/domain/types.ts`; the HTTP adapter is `services/ride-api-service.ts`; the socket adapter is `realtime/websocket-realtime-client.ts`.

Items marked **Assumption** are frontend guesses that need a product or backend decision. Nothing here is business policy until confirmed.

## 0. Conventions

- **Base URL:** `NEXT_PUBLIC_PAID_RIDES_API_BASE_URL` (e.g. `https://api.example.com/api/v1`). Realtime: `NEXT_PUBLIC_PAID_RIDES_REALTIME_URL` (e.g. `wss://api.example.com/rides/ws`).
- **Auth:** the same HTTP-only session cookie the rest of K-Rides uses; the browser sends `credentials: "include"`. CORS must allow the app origin with credentials, and cookies must be `SameSite=None; Secure` if the API is cross-site. No bearer tokens in JavaScript.
- **JSON** everywhere; timestamps are ISO-8601 UTC, **server-generated**.
- **Money:** `{ "amountMinor": 190000, "currency": "NGN" }` — integer minor units, never floats.
- **Coordinates:** `{ "lat": 9.0647, "lng": 7.4728 }` (WGS84 decimal degrees).
- **Idempotency:** every mutation carries `Idempotency-Key: <uuid or derived key>`. The server must return the original result for a repeated key (same user, same endpoint) for at least 24 hours, and never perform the action twice. Keys the frontend derives: `accept:<offerId>`, `decline:<offerId>`, `cancel:<rideId>`, `arrive:<rideId>`, `start:<rideId>`, `complete:<rideId>`, `rate:<rideId>`, `sos:<rideId>:<type>`; ride requests and messages use a random UUID reused across retries of the same attempt.
- **Errors:** non-2xx with `{ "code": "<code>", "message": "<for logs, not shown to users>" }`. Codes the frontend understands: `estimate_expired`, `no_drivers`, `already_taken`, `request_expired`, `driver_ineligible`, `payment_failed`, `conflict`. Otherwise HTTP status maps to: 400/422 validation, 401 unauthorized, 403 forbidden, 404 not_found, 409 conflict, 410 request_expired, 429 rate_limited, 501 not_supported, 5xx unavailable.
- **Versioning:** every ride has a `version` integer that increases on every change to that ride (status, driver, location-derived ETA, rating…). The frontend discards anything not newer than what it holds.

## 1. Entities

| Entity | Purpose | Key fields |
| --- | --- | --- |
| Passenger | a member who books | id (never sent to other users), displayName, avatarUrl, rating, verified membership |
| Driver | a verified member who drives | id, displayName (approved public name), avatarUrl, rating, completedRides, verification status, eligible ride classes |
| DriverAvailability | live driver state | driverId, `OFFLINE \| ONLINE \| ON_TRIP`, lastLocation, updatedAt |
| Vehicle | approved car | make, model, color, colorHex (optional), plate, vehicleClass `compact \| sedan \| suv \| van \| premium`, capacity |
| RideOption (product) | what riders choose | id, name, description, vehicleClass, capacity, shared, availability rules — **data-driven, not hard-coded** |
| FareEstimate | priced options for a route | estimateId, expiresAt, route, options[] (each with FareQuote and pickupEtaSeconds), paymentMethods[], defaultPaymentMethodId |
| Ride | the trip | see §2.3 |
| RideRequest / Offer | a ride offered to one driver | offerId, rideId, driverId, expiresAt, status `offered \| accepted \| declined \| expired \| withdrawn` |
| RideParticipant | riders on a shared trip | rideId, passengerId, seat count, pickup/drop-off order, boarded |
| LocationUpdate | a driver fix | driverId, rideId?, lat, lng, heading, speed, accuracy, recordedAt (device), receivedAt (server) |
| Route | road geometry | points[], distanceMeters, durationSeconds, source `road \| approximate` |
| RideMessage | chat | id, clientId, rideId, sender role, body (≤ 500 chars), sentAt, deliveredAt, readAt |
| Rating | after the trip | rideId, raterRole, stars 1–5, tags[], comment? |
| SafetyEvent / SOS | emergencies and reports | id, rideId, reporter, type, location, occurredAt, status `sent \| acknowledged \| resolved`, handler |
| PaymentReference | settlement | rideId, method, status `not_required \| pending \| authorized \| paid \| failed`, amount |

## 2. API contracts

### 2.1 Viewer

`GET /rides/me` → `{ displayName, avatarUrl?, roles: ["passenger"] | ["passenger","driver"], vehicle?: Vehicle }`. 401 when signed out. The frontend hides driver screens without `"driver"` but the server must enforce roles on every driver endpoint.

### 2.2 Places and routing

Handled today by the app's own proxies to OpenStreetMap services (`/api/rides/geo/search|reverse|route`). If the backend prefers to own geocoding (billing, caching, a commercial provider), expose `GET /places/search?q=&lat=&lng=` → `Place[]`, `GET /places/reverse?lat=&lng=` → `Place`, and include the route in the estimate; the frontend changes one provider file.

### 2.3 Ride projection

Every ride-returning endpoint and `ride.updated` event returns this role-scoped projection:

```json
{
  "id": "ride_8f2c", "version": 14, "status": "DRIVER_EN_ROUTE",
  "createdAt": "…", "updatedAt": "…",
  "pickup":      { "id": "…", "name": "Wuse Market", "secondary": "Douala Street, Wuse", "point": { "lat": 9.0647, "lng": 7.4728 }, "source": "search" },
  "destination": { "…": "same shape" },
  "route": { "points": [{ "lat": 0, "lng": 0 }], "distanceMeters": 5900, "durationSeconds": 540, "source": "road" },
  "option": { "id": "prod-go", "name": "Go", "vehicleClass": "compact", "capacity": 4, "shared": false },
  "fare": { "total": { "amountMinor": 190000, "currency": "NGN" }, "range": { "low": {}, "high": {} }, "breakdown": [{ "code": "base", "label": "Base fare", "amount": {} }], "isEstimate": true },
  "payment": { "status": "pending", "methodLabel": "Cash" },
  "driver":    { "displayName": "Emeka", "avatarUrl": "…", "rating": 4.92, "completedRides": 1284, "verified": true,
                 "vehicle": { "make": "Toyota", "model": "Corolla", "color": "Silver", "colorHex": "#c9ccce", "plate": "ABJ 482 KR", "vehicleClass": "compact" },
                 "contact": { "call": "masked", "callHref": "tel:+234…", "chat": true } },
  "passenger": "present only in the driver's projection: { displayName, avatarUrl?, rating?, contact }",
  "driverEarning": "driver projection only: Money",
  "driverLocation": { "point": {}, "heading": 182, "speedMps": 8.1, "accuracyMeters": 6, "recordedAt": "…" },
  "approach": { "etaSeconds": 240, "distanceMeters": 1800, "route": "optional Route driver → pickup" },
  "progress": { "etaSeconds": 420, "remainingMeters": 3900 },
  "shared": { "riderCount": 2, "seatsAvailable": 0, "coRiders": [{ "alias": "Rider 2", "boarded": false }], "mayDetour": true, "maxExtraSeconds": 360 },
  "cancellation": { "by": "driver", "reason": "…", "fee": {} },
  "timestamps": { "assignedAt": "…", "arrivedAt": "…", "startedAt": "…", "completedAt": "…", "cancelledAt": "…" },
  "sos": { "status": "none" },
  "rating": { "stars": 5, "submittedAt": "…" },
  "shareUrl": "optional live-tracking URL",
  "tripPin": "optional, passenger projection only, when PINs are enabled"
}
```

Rules: never include internal user ids, raw phone numbers (unless the product explicitly allows `call: "direct"`), another rider's identity on shared trips, or payment card details. ETA and distance must be **route-aware** (from the routing engine), never straight-line.

### 2.4 Passenger endpoints

| Operation | Request | Success | Failures |
| --- | --- | --- | --- |
| Current ride | `GET /rides/current` | `{ "ride": Ride \| null }` — the rider's live ride; also a just-finished ride (completed and unrated, or cancelled) for a short window so a reload can show the receipt. **Assumption:** 15 min for unrated completions, 2 min for cancellations. | 401 |
| Ride | `GET /rides/{rideId}` | `Ride` | 403 if not a participant, 404 |
| Estimate | `POST /rides/estimate` `{ pickup: Place, destination: Place }` | `FareEstimate` | 422 invalid/unserviceable points; 503 pricing unavailable |
| Request | `POST /rides/request` `{ estimateId, optionId, paymentMethodId, pickup, destination }` + Idempotency-Key | `Ride` with status `REQUESTED` or `SEARCHING` | 409 `conflict` (already has a live ride — the frontend re-reads current), 410/`estimate_expired`, 402 `payment_failed`, 422 |
| Retry search | `POST /rides/{rideId}/retry` `{ optionId \| null }` | `Ride` (`SEARCHING`) | 409 if not `NO_DRIVERS` |
| Cancel | `POST /rides/{rideId}/cancel` `{ reason }` | `Ride` (`CANCELLED`, with `cancellation.fee` if any) | 409 if already in progress/finished |
| Rate | `POST /rides/{rideId}/rating` `{ stars, tags[], comment? }` | `Ride` with `rating` | 409 already rated, 422 |
| History | `GET /rides/history?cursor=` | `{ items: RideSummary[], nextCursor }` | — |

`RideSummary`: `{ id, status, createdAt, pickupName, destinationName, optionName, vehicleClass, total: Money|null, distanceMeters, durationSeconds|null, counterpartName?, ratingGiven? }`.

### 2.5 Driver endpoints

| Operation | Request | Success | Failures |
| --- | --- | --- | --- |
| Availability | `GET /driver/availability` | `{ availability }` | 403 not a driver |
| Go online/offline | `PUT /driver/availability` `{ online }` | `{ availability }` | 403 unverified/suspended; 409 going offline while `ON_TRIP` |
| Current ride | `GET /driver/rides/current` | `{ ride: Ride \| null }` (same just-finished window) | |
| Accept | `POST /driver/offers/{offerId}/accept` | `Ride` (`DRIVER_ASSIGNED`/`DRIVER_EN_ROUTE`) | 409 `already_taken`, 410 `request_expired`, 403 `driver_ineligible` |
| Decline | `POST /driver/offers/{offerId}/decline` | 204 | 410 (ignored by the client) |
| Arrived | `POST /driver/rides/{rideId}/arrived` | `Ride` (`DRIVER_ARRIVED`) | 409 invalid state. **Assumption:** server may reject if the driver is far from pickup; the client already warns. |
| Start | `POST /driver/rides/{rideId}/start` `{ pin? }` | `Ride` (`IN_PROGRESS`) | 409, 422 wrong PIN |
| Complete | `POST /driver/rides/{rideId}/complete` | `Ride` (`COMPLETED`, final fare, payment status) | 409 |
| Driver cancel | `POST /driver/rides/{rideId}/cancel` `{ reason }` | `Ride` (rider re-dispatched or cancelled) | 409 after start |
| History | `GET /driver/rides/history?cursor=` | as rider history, `total` = driver earning | |
| Location (REST fallback) | `POST /driver/location` `LiveLocation` | 204 | 429 |

### 2.6 Chat

| Operation | Request | Success |
| --- | --- | --- |
| List | `GET /rides/{rideId}/messages` | `{ items: RideMessage[] }` (oldest first) |
| Send | `POST /rides/{rideId}/messages` `{ clientId, body }` + Idempotency-Key | `RideMessage` echoing `clientId` |
| Read | `POST /rides/{rideId}/messages/read` `{ lastMessageId }` | 204 |

Chat is open from assignment until the trip ends (**Assumption:** read-only afterwards). Max 500 characters; plain text. Messages are visible only to the ride's participants.

### 2.7 Safety

| Operation | Request | Success |
| --- | --- | --- |
| SOS | `POST /rides/{rideId}/sos` `{ type: "medical"\|"accident"\|"threat"\|"other", location: LatLng\|null, occurredAt }` + Idempotency-Key | 202; later `ride.sos.acknowledged` |
| Report issue | `POST /rides/{rideId}/safety-reports` `{ category, details? }` | 202 |
| Share link | `POST /rides/{rideId}/share-link` | `{ url \| null }` — a revocable, expiring, read-only live link |

SOS must be logged immutably, routed to a human safety desk, include both parties' last locations server-side, and never be rate-limited away. The frontend will **not** call `/sos` until the backend confirms it exists (`NEXT_PUBLIC_PAID_RIDES_SOS_ENDPOINT=on`).

## 3. Realtime events

### 3.1 Connection

`wss://…/rides/ws`, authenticated by the session cookie at upgrade. Client frames:

| Frame | Meaning |
| --- | --- |
| `{ "type": "subscribe", "channel": "ride:<rideId>" }` | rider or assigned driver only — reject anyone else |
| `{ "type": "subscribe", "channel": "driver" }` | the current user's driver channel (offers, availability) |
| `{ "type": "unsubscribe", "channel": "…" }` | |
| `{ "type": "location", "location": LiveLocation }` | driver fix (see §5) |
| `{ "type": "typing", "rideId": "…", "typing": true }` | chat typing, rate-limited |
| `{ "type": "ping" }` → `{ "type": "pong" }` | heartbeat every 25 s |

Server frames: `{ "type": "event", "event": RideEvent }` where

```json
{ "id": "evt_01H…", "rideId": "ride_8f2c", "seq": 1042, "rideVersion": 15, "occurredAt": "…", "payload": { "type": "ride.updated", "ride": { } } }
```

- `id` unique per event (the client de-duplicates); `seq` monotonic per connection or per channel (gaps trigger a re-read); `rideVersion` the version produced.
- Delivery is at-least-once. The client tolerates duplicates and reordering. After any reconnect it re-reads `GET /rides/current`; **do not rely on replay**.

### 3.2 Events

| Event | Sender → recipient | Example payload | When | Frontend response | Idempotency |
| --- | --- | --- | --- | --- | --- |
| `ride.updated` | server → rider, driver | `{ ride }` | every status change: requested, search started, assigned, en route, arriving, arrived, started, paused, destination updated, cancelled, completed, rating, payment | applies if `version` is newer; drives the phase | version-gated |
| `ride.driver.location_updated` | server → rider (and driver echo) | `{ location, approach?, progress? }` | each accepted driver fix, ≤ 1 per 2–4 s | tweens the car, updates ETA/distance | `recordedAt` must increase |
| `driver.request.received` | server → driver | `{ offer: RideOffer }` | ride offered to this driver | shows the request sheet + countdown | `offerId` |
| `driver.request.expired` | server → driver | `{ offerId }` | offer timed out | "This request expired" (ignored while an accept is in flight) | `offerId` |
| `driver.request.withdrawn` | server → driver | `{ offerId, reason: "taken" \| "cancelled" }` | rider cancelled / another driver won | "Another driver got this one" | `offerId` |
| `driver.availability.changed` | server → driver | `{ availability }` | server-side change (suspension, trip end) | updates Online/Offline | — |
| `chat.message.created` | server → other participant (and sender echo with `clientId`) | `{ message }` | new message | inserts/reconciles bubble, unread badge | message `id` / `clientId` |
| `chat.message.delivered` | server → sender | `{ messageId }` | delivered to the other device | ✓✓ | forward-only |
| `chat.message.read` | server → sender | `{ messageId }` | read | ✓✓ blue | forward-only |
| `chat.typing.started` / `stopped` | server → other participant | `{}` | relayed typing | typing indicator | — |
| `ride.sos.acknowledged` | server → reporter | `{}` | safety desk picked it up | "The safety team has your alert" | — |

Suggested names map to the brief's list: `ride.requested`, `ride.search.started`, `ride.driver.assigned`, `ride.driver.arriving`, `ride.driver.arrived`, `ride.started`, `ride.destination_updated`, `ride.cancelled`, `ride.completed` are all delivered as `ride.updated` with the corresponding status; a backend may emit the specific names as long as each carries the full ride. `connection.reconnecting/restored` are client-side states, not server events.

## 4. Matching requirements

- Configurable search radius per city/product. **Assumption:** start at ~5 km and widen in steps up to a cap.
- Only drivers who are `ONLINE`, verified, not suspended, with an eligible vehicle class and enough seats; shared rides only to drivers opted into sharing, respecting the 3-passenger cap the product already uses.
- Offer one driver at a time (or a small batch) with an `expiresAt`. **Assumption:** 15–20 s.
- **Atomic assignment:** accepting must lock the ride (row lock / compare-and-set on `status = SEARCHING AND driverId IS NULL`). Exactly one accept wins; the others get `409 already_taken` and `driver.request.withdrawn`.
- Expiry and decline move to the next candidate; no candidate within the cap → `NO_DRIVERS` (rider may retry).
- A driver cancelling after assignment re-dispatches (`SEARCHING`, bumped version) unless the product decides otherwise.
- Never offer the same ride twice to a driver who declined it in this dispatch.

## 5. Location requirements

- Drivers send fixes over the socket (`location` frame), REST as fallback. The client throttles: at most every 4 s and only after ≥ 15 m of movement, with a 20 s heartbeat. The server may tighten this per ride and should drop fixes faster than ~1 Hz.
- Store the latest fix per driver with `recordedAt` (device) and `receivedAt` (server). Reject fixes older than the stored one; treat a driver with no fix for **Assumption:** 60 s as stale (show last known position, don't match).
- Accuracy above ~100 m should not drive arrival logic.
- ETA and remaining distance come from the routing engine on the current route, recomputed on meaningful movement, and are included in location events.
- Keep raw location history only as long as policy requires (safety investigations), access-controlled and audited; never expose it to the other party beyond the live position during the trip.

## 6. Security

The backend must:

- Authenticate every HTTP request and the socket upgrade.
- Authorise ride subscriptions: only the ride's rider(s) and assigned driver may subscribe to `ride:<id>`; only the driver to `driver`.
- Prevent drivers spoofing updates for rides that aren't theirs (location, arrived, start, complete).
- Validate every state transition against the table in §7; reject anything else with 409.
- Make ride assignment atomic and all mutations idempotent.
- Use server timestamps for every state change.
- Rate-limit: ride requests, cancellations, offer responses, chat, typing, location, share-link creation, safety reports (never SOS).
- Protect private data: role-scoped projections, masked calling, no raw phone numbers, no internal ids, anonymised co-riders.
- Log safety events immutably and alert a human.
- Issue share links that are random, expiring and revocable, exposing only first name, car, plate, live position and ETA.

## 7. State machine

```
REQUESTED ──► SEARCHING ──► DRIVER_ASSIGNED ──► DRIVER_EN_ROUTE ⇄ DRIVER_ARRIVING ──► DRIVER_ARRIVED ──► IN_PROGRESS ⇄ PAUSED ──► COMPLETED
    │             │  ▲              │                  │                 │                  │                 │
    │             ▼  │              └──── re-dispatch (driver cancels) ──┴──► SEARCHING      │                 │
    │         NO_DRIVERS ── retry ──┘                                                        │                 │
    └──────────── any state before COMPLETED ──────────────────────────────────────────────────────► CANCELLED
```

| From | Allowed to |
| --- | --- |
| REQUESTED | SEARCHING, NO_DRIVERS, CANCELLED |
| SEARCHING | DRIVER_ASSIGNED, NO_DRIVERS, CANCELLED |
| NO_DRIVERS | SEARCHING (retry), CANCELLED |
| DRIVER_ASSIGNED | DRIVER_EN_ROUTE, DRIVER_ARRIVING, DRIVER_ARRIVED, SEARCHING (re-dispatch), CANCELLED |
| DRIVER_EN_ROUTE | DRIVER_ARRIVING, DRIVER_ARRIVED, SEARCHING, CANCELLED |
| DRIVER_ARRIVING | DRIVER_EN_ROUTE, DRIVER_ARRIVED, SEARCHING, CANCELLED |
| DRIVER_ARRIVED | IN_PROGRESS, CANCELLED |
| IN_PROGRESS | PAUSED, COMPLETED, CANCELLED (exceptional, e.g. safety) |
| PAUSED | IN_PROGRESS, COMPLETED, CANCELLED |
| COMPLETED, CANCELLED | — (terminal; payment and rating are fields, not statuses) |

`DRIVER_ARRIVING` is a server-side threshold on route distance/ETA (**Assumption:** ~300 m or 60 s). Payment failure is `COMPLETED` with `payment.status = "failed"`.

## 8. Open decisions for product/backend

1. Cancellation fees: when they apply and how they are shown before confirming.
2. Whether drivers see the destination before accepting (the UI supports both).
3. Trip PIN: on or off (`tripPinRequired`).
4. Masked calling provider (or chat-only at launch).
5. Payment methods at launch (cash only vs. card/transfer) and how failed payments are settled.
6. Offer timeout, radius steps and cap.
7. Retention period for location history and chat.
8. SOS desk operations and the acknowledgement SLA.
