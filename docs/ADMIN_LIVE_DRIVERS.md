# Admin Live Drivers API

Returns every driver who is **currently online** in the dispatch system, along with their latest
GPS position. Use it to power a fleet map in the admin dashboard.

| | |
| --- | --- |
| **Method** | `GET` |
| **Path** | `/api/v1/admin/drivers/live` |
| **Auth** | Access JWT with `Admin` or `Root` role |
| **Query params** | None |
| **Pagination** | None — all live drivers are returned in one response |
| **Swagger** | `/swagger-ui` → tag **Paid Rides Admin** |

---

## Authentication

Send either:

- `Authorization: Bearer <JWT_ACCESS_TOKEN>`, or
- the existing `access_token` HTTP-only cookie (browser requests with `credentials: "include"`).

---

## What counts as "live"

A driver appears in this list when **all** of the following are true:

1. They have sent a location ping (via `POST /api/v1/driver/location` or the driver WebSocket)
   within the last **15 seconds**.
2. They have not gone offline (`PUT /api/v1/driver/availability`) or been deactivated by an admin.
3. Their user account is active (`is_active = true`).

Drivers drop off the list automatically ~15 seconds after their last ping (e.g. app closed,
lost connectivity).

---

## Request

```http
GET /api/v1/admin/drivers/live HTTP/1.1
Host: api.freebus.cloud
Authorization: Bearer eyJhbGciOi...
```

```bash
curl -H "Authorization: Bearer $TOKEN" https://api.freebus.cloud/api/v1/admin/drivers/live
```

---

## Response `200 OK`

```json
{
  "items": [
    {
      "driverId": "0192f3a1-7c2e-7b10-9a4d-3e5f6a7b8c9d",
      "displayName": "John Doe",
      "phone": "+2348012345678",
      "lat": 9.076479,
      "lng": 7.398574,
      "heading": 120.5,
      "speedMps": 8.2,
      "availableSeats": 4,
      "lastSeenAt": "2026-10-19T17:25:41.123+00:00",
      "currentRide": {
        "rideId": "0193a2b4-1d2e-7f30-8b4c-5d6e7f8a9b0c",
        "status": "IN_PROGRESS"
      }
    },
    {
      "driverId": "0192f3a1-8d3f-7c21-ab5e-4f6a7b8c9d0e",
      "displayName": "Jane Smith",
      "phone": "+2348098765432",
      "lat": 9.057180,
      "lng": 7.489690,
      "heading": null,
      "speedMps": null,
      "availableSeats": 4,
      "lastSeenAt": "2026-10-19T17:25:38.902+00:00",
      "currentRide": null
    }
  ],
  "total": 2
}
```

When no drivers are online:

```json
{ "items": [], "total": 0 }
```

### Fields

| Field | Type | Nullable | Description |
| --- | --- | --- | --- |
| `items` | array | no | Live drivers, sorted by name (first, then last). |
| `total` | integer | no | Number of items returned. |
| `items[].driverId` | UUID string | no | Driver's user ID. Use with `GET /admin/drivers/{id}` for full details. |
| `items[].displayName` | string | no | `first_name last_name`. |
| `items[].phone` | string | no | Driver's phone number. |
| `items[].lat` | number | no | Latest latitude (WGS84, decimal degrees). |
| `items[].lng` | number | no | Latest longitude (WGS84, decimal degrees). |
| `items[].heading` | number | yes | Direction of travel in degrees (0–360, 0 = north). From the last REST location update; may be `null`. |
| `items[].speedMps` | number | yes | Speed in metres per second (× 3.6 for km/h). May be `null`. |
| `items[].availableSeats` | integer | no | Seats currently available for dispatch. |
| `items[].lastSeenAt` | ISO-8601 string | no | Time of the driver's last location ping (UTC). |
| `items[].currentRide` | object | yes | Driver's active ride, or `null` if idle. |
| `items[].currentRide.rideId` | UUID string | no | Use with `GET /admin/rides/{id}` for ride details. |
| `items[].currentRide.status` | string | no | One of `DRIVER_ASSIGNED`, `DRIVER_EN_ROUTE`, `DRIVER_ARRIVING`, `DRIVER_ARRIVED`, `IN_PROGRESS`, `PAUSED`. |

> **Map marker tip:** `currentRide === null` → idle/available driver; otherwise → busy driver.
> Use `currentRide.status` to colour-code markers if needed.

---

## Errors

All admin errors use the `{ code, message }` envelope.

| Status | `code` | When |
| --- | --- | --- |
| `401` | — (existing JWT rejection shape) | Missing, invalid or expired token. |
| `403` | `forbidden` | Authenticated user is not `Admin` or `Root`. |
| `503` | `dispatch_unavailable` | Live driver index (Redis) could not be read. |
| `503` | `unavailable` | Database unavailable. |

```json
{ "code": "forbidden", "message": "Admin or Root role required" }
```

---

## Integration guide

### Polling

Positions update every few seconds and drivers expire after 15 s, so **poll every 5–10 seconds**
while the map is visible. Stop polling when the tab/page is hidden.

```ts
type LiveDriver = {
  driverId: string;
  displayName: string;
  phone: string;
  lat: number;
  lng: number;
  heading: number | null;
  speedMps: number | null;
  availableSeats: number;
  lastSeenAt: string;
  currentRide: { rideId: string; status: string } | null;
};

type LiveDriversResponse = { items: LiveDriver[]; total: number };

const API = "https://api.freebus.cloud/api/v1";

export async function fetchLiveDrivers(): Promise<LiveDriversResponse> {
  const res = await fetch(`${API}/admin/drivers/live`, { credentials: "include" });
  if (!res.ok) throw new Error(`Live drivers failed: ${res.status}`);
  return res.json();
}

export function pollLiveDrivers(
  onData: (data: LiveDriversResponse) => void,
  intervalMs = 5000,
): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout>;

  const tick = async () => {
    if (stopped) return;
    if (document.visibilityState === "visible") {
      try {
        onData(await fetchLiveDrivers());
      } catch (e) {
        console.error(e);
      }
    }
    timer = setTimeout(tick, intervalMs);
  };

  tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}
```

### Updating markers

Key markers by `driverId`. On each poll:

1. Add markers for new `driverId`s.
2. Move existing markers to the new `lat`/`lng` (optionally animate and rotate by `heading`).
3. Remove markers whose `driverId` is no longer in `items` — that driver went offline.

### Related endpoints

| Method | Path | Use |
| --- | --- | --- |
| GET | `/api/v1/admin/drivers/{id}` | Full driver profile when a marker is clicked. |
| GET | `/api/v1/admin/rides/{id}` | Ride details for `currentRide.rideId`. |
| GET | `/api/v1/admin/drivers` | Full driver roster (online and offline, paginated). |

---

## Notes

- Positions come from the in-memory dispatch index (Redis) and are the same data used to match
  riders with drivers, so the map reflects what dispatch "sees".
- Drivers who were already online when this feature was deployed appear after their next ping
  (within ~15 seconds).
- The response is not paginated. It is intended for a single city fleet; if the fleet grows into
  the thousands, a bounding-box filter can be added.
