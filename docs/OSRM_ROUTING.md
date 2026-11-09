# Backend OSRM routing

The backend uses `https://app.freebus.cloud/routing` by default. Copy the settings
in [routing.env.example](../routing.env.example) into deployment configuration.
`OSRM_URL` overrides the primary base URL. On the routing VPS, set it to the actual
private service URL to avoid public DNS, TLS and nginx limits. No private address
or port was supplied, so the backend does not assume one. Loopback is unsuitable
from another server or an isolated container network.

## Request and route validation

The shared client requests only:

```text
GET {OSRM_URL}/route/v1/driving/{lng},{lat};{lng},{lat}?overview=full&geometries=geojson&steps=false&radiuses=1000;1000
```

It requires a successful HTTP status, `code: "Ok"`, positive finite route distance
and duration, and valid GeoJSON LineString coordinates. Values stay in metres and
seconds. Response bodies are limited to 4 MiB. Invalid or missing geometry cannot
be replaced with a straight line. There are no Google or synthetic fare fallbacks.

Fare estimates use road distance/duration and the existing backend pricing rules.
Routing failure does not save a fare estimate or produce a zero/free fare.
Immediate and scheduled bookings reject legacy approximate/zero-route estimates;
the passenger must request a fresh estimate. Existing booked fares stay locked.

## Performance and failure policy

One process-wide pooled client serves fare routing and driver ranking:

- Each upstream request has a 3.5-second timeout. Waiting for the shared request
  gate is separately bounded at 3.5 seconds; backup requests have their own bounds.
- Successful directed coordinate pairs are cached for 60 seconds, up to 512 routes.
  Concurrent identical misses are coalesced. Coordinates are normalized to six decimals.
- Upstream request starts are paced at least 200 ms apart, without burst traffic.
  This pacing is shared within one process, not between replicas or frontend clients.
  Multiple clients behind one source IP can still receive 429; deploy private routing
  or coordinate aggregate capacity when scaling.
- Network errors, timeouts, HTTP 429/5xx, and malformed/invalid responses open a
  30-second per-provider outage cooldown and can select the configured backup.
- Real `NoRoute`/`NoSegment` responses do not trigger backup. Valid client-error
  responses do not trigger backup either.
- `ROUTING_FALLBACK_API_BASE_URL` enables an optional OSRM-compatible backup.
  No backup is configured by default. Both known public demo hosts are rejected,
  including when configured as the primary. Keep backup coverage/profile consistent.
- No routing calls were added to GPS or WebSocket heartbeat processing.

Driver matching uses Redis/H3 spatial distance to shortlist the five closest nearby
candidates, then obtains **driver-to-pickup** road routes through the route endpoint.
Only successfully routed candidates within the road-distance radius are ranked by
ETA. The shortlist has a five-second total routing budget. Unroutable candidates
are skipped; a service outage stops the remaining shortlist. This deliberately bounds
traffic/latency and can omit a suitable driver outside the five-candidate shortlist.
No table/match/trip service is called. Pure spatial fleet queries remain available
for geographic discovery; their distances do not generate fares or road ETAs.

## Frontend-facing errors

`POST /api/v1/rides/estimate` can now return:

```json
{
  "code": "route_not_available",
  "message": "No road route available for these locations"
}
```

HTTP **422**: ask the passenger to select different locations. This includes an
unsnappable endpoint or a disconnected road route; it does not prove FCT residency.

```json
{
  "code": "routing_unavailable",
  "message": "Routing temporarily unavailable; please retry"
}
```

HTTP **503**, with `Retry-After: 30`: show temporary unavailability and allow a later
retry. Do not display a straight-line fare. Invalid configured provider URLs also
fail closed with this response.

Driver lookup can return no routed candidates during an outage even when a route
quote was cached. An estimate's `available: false` is not a guarantee that no
eligible driver exists. Future scheduled requests can still be booked from a valid
road estimate and matched later through the scheduled-request board.

## Coverage and deployment limits

The supplied graph covers the requested FCT/NG-FC operating area, retaining boundary
roads and restrictions for graph integrity. Snapping is capped at 1,000 metres.
Road coverage is not a legal endpoint geofence. No dataset polygon was supplied to
this repository, so polygon-based residency validation is not implemented.

A backup on the same VPS cannot survive a whole-VPS outage. No online provider can
serve an offline passenger or driver. HTTPS access has no API key in the supplied
service configuration; keep private service exposure under deployment control.

## Verification and references

`cargo test --offline --lib` includes local HTTP fixtures for coordinate order,
metric units, caching/coalescing, 429/5xx cooldown, coverage failures, invalid routes,
blocked demo URLs, connection failures, and real request timeout/failover. The local
fixtures need permission to bind loopback sockets. The database/Redis integration
tests retain their existing explicit opt-in requirements.

The request/response contract follows the [OSRM HTTP API](https://github.com/Project-OSRM/osrm-backend/blob/master/docs/http.md).
Public demos are excluded under the [OSRM demo usage policy](https://github.com/Project-OSRM/osrm-backend/wiki/Demo-server).
