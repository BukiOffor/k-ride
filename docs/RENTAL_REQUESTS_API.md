# Rental API integration guide

## Overview

Customers request a car hire for at least 24 consecutive hours. An administrator arranges the car offline, then marks the request as assigned in the system.

This API records requests and assignment status. It does not provide vehicle selection, driver assignment, availability checks, prices, payments, or car/driver details. The airport fare rule for ride bookings does not apply to rental requests.

## Base URL and authentication

Use your deployed API origin followed by `/api/v1`:

```text
https://<your-api-host>/api/v1
```

Local development uses `http://localhost:9990/api/v1`. All endpoint paths below are relative to this base URL.

Every request requires the existing login session. Send the access token as:

```http
Authorization: Bearer <access_token>
```

The existing `access_token` cookie is also supported. Browser clients using cookie authentication must send credentials and use an origin permitted by the server's CORS configuration.

For POST and PUT requests with a JSON body, also send:

```http
Content-Type: application/json
```

User routes operate on the authenticated user's own records. Admin routes require an `Admin` or `Root` role. Even an administrator using `/rentals` sees only their own requests; use `/admin/rentals` for the network-wide list.

## Endpoints

| Method | Path | Access | Success |
| --- | --- | --- | --- |
| POST | `/rentals` | Authenticated user | `201` rental object |
| GET | `/rentals` | Authenticated user | `200` array of owned rentals |
| GET | `/rentals/{id}` | Owner | `200` rental object |
| PUT | `/rentals/{id}` | Owner, pending only | `200` updated rental object |
| DELETE | `/rentals/{id}` | Owner, pending only | `204` empty body |
| GET | `/admin/rentals` | Admin/Root | `200` array of all matching rentals |
| GET | `/admin/rentals/{id}` | Admin/Root | `200` rental object |
| POST | `/admin/rentals/{id}/assign` | Admin/Root | `200` assigned rental object |

`{id}` must be the UUID returned when the request was created. Successful JSON responses contain the object or array directly, without a `data` or `message` wrapper.

## Rental object

Example pending request:

```json
{
  "id": "019a3e00-0000-7000-8000-000000000001",
  "userId": "019a3e00-0000-7000-8000-000000000002",
  "startsAt": "2026-12-01T09:00:00Z",
  "endsAt": "2026-12-02T09:00:00Z",
  "pickupLocation": "Abuja city centre",
  "notes": "Car for four passengers with luggage",
  "status": "pending",
  "assignedBy": null,
  "assignedAt": null,
  "createdAt": "2026-11-01T12:00:00Z",
  "updatedAt": "2026-11-01T12:00:00Z"
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | UUID string | Rental request identifier |
| `userId` | UUID string | Requesting user's ID, set from authentication |
| `startsAt` | RFC 3339 timestamp | Hire start instant |
| `endsAt` | RFC 3339 timestamp | Hire end instant |
| `pickupLocation` | string | Free-text pickup location; outer whitespace is trimmed |
| `notes` | string | Customer requirements; empty string when omitted |
| `status` | `pending` or `assigned` | Current request status |
| `assignedBy` | UUID string or null | Administrator who marked the request assigned; this is not a driver ID |
| `assignedAt` | timestamp or null | Time assignment was recorded |
| `createdAt` | timestamp | Creation time |
| `updatedAt` | timestamp | Last successful change time |

Send timestamps with a timezone, preferably UTC using `Z`. Display them in the customer's local timezone. The examples use fixed dates for illustration; actual requests must start in the future.

## Create a request

```http
POST /api/v1/rentals
Authorization: Bearer <access_token>
Content-Type: application/json
```

```json
{
  "startsAt": "2026-12-01T09:00:00Z",
  "endsAt": "2026-12-02T09:00:00Z",
  "pickupLocation": "Abuja city centre",
  "notes": "Car for four passengers with luggage"
}
```

Returns `201 Created` with the pending rental object above.

| Input | Required | Validation |
| --- | --- | --- |
| `startsAt` | Yes | Valid timestamp strictly later than the server's current time |
| `endsAt` | Yes | Valid timestamp at least 24 hours after `startsAt` |
| `pickupLocation` | Yes | Nonblank string; maximum 1,000 characters before trimming |
| `notes` | No | String, maximum 5,000 characters; defaults to `""` |

Exactly 24 hours is accepted. A hire spanning two calendar dates but lasting less than 24 hours is rejected. Duration is measured between timestamp instants, including their timezone offsets.

Do not send `id`, `userId`, `status`, assignment fields, or any other extra fields. Unknown fields are rejected. `notes: null` is invalid; omit it or send a string.

Creation has no idempotency-key support. Disable duplicate submissions while a request is in flight. If a network interruption makes the outcome uncertain, refresh the user's list before retrying to reduce duplicate requests.

## List requests

Customer list:

```http
GET /api/v1/rentals?status=pending&limit=20&offset=0
```

Admin list:

```http
GET /api/v1/admin/rentals?status=pending&limit=20&offset=0
```

Both return `200 OK` with an array of rental objects. No results returns `[]`.

| Parameter | Default | Allowed values |
| --- | --- | --- |
| `status` | All statuses | `pending` or `assigned`, lowercase |
| `limit` | `50` | Integer from `1` to `100` |
| `offset` | `0` | Nonnegative integer |

All parameters are optional. Omit `status` to see both statuses. Unknown query parameters are rejected.

Results are ordered by `createdAt` descending, then ID descending. To fetch the next page, increase `offset` by `limit`. A page shorter than `limit` means there were no further matching records at that moment. There is no total count or next-page cursor; a full page may be followed by an empty page. Refresh from offset zero when filters change. New requests or deletions can shift offset-based pages.

## Read one request

Customer detail:

```http
GET /api/v1/rentals/{id}
```

Admin detail:

```http
GET /api/v1/admin/rentals/{id}
```

Returns `200 OK` with one rental object. A user requesting another user's rental receives `404`, as does a request for a nonexistent ID. The admin endpoint can read any rental and includes its `userId`; customer names and contact details are not embedded.

## Update a pending request

```http
PUT /api/v1/rentals/{id}
Content-Type: application/json
```

```json
{
  "startsAt": "2026-12-03T10:00:00Z",
  "endsAt": "2026-12-05T10:00:00Z",
  "pickupLocation": "Hotel reception, Abuja",
  "notes": "Please allow room for two suitcases"
}
```

Returns `200 OK` with the updated rental object. Send all required creation fields: this is a replacement of editable details, not a partial PATCH. Omitting `notes` resets it to an empty string. The start must still be in the future and the duration must still be at least 24 hours.

Only the owner can update, and only while `status` is `pending`. Assigned requests return `409 Conflict`. Admin routes do not provide an edit operation.

## Delete a pending request

```http
DELETE /api/v1/rentals/{id}
```

Returns `204 No Content`. Do not attempt to parse a JSON response body. This permanently deletes the pending record; it does not create a `cancelled` status. Repeating deletion returns `404`. Assigned requests return `409`.

## Mark a rental assigned

After arranging the car offline, an Admin/Root caller sends:

```http
POST /api/v1/admin/rentals/{id}/assign
Authorization: Bearer <admin_access_token>
```

Send no request body. No car ID or driver ID is required or recorded.

Returns `200 OK` with the full rental object. Its assignment fields now look like:

```json
{
  "status": "assigned",
  "assignedBy": "019a3e00-0000-7000-8000-000000000003",
  "assignedAt": "2026-11-01T12:30:00Z",
  "updatedAt": "2026-11-01T12:30:00Z"
}
```

The JSON above is an excerpt, not the complete response. Repeating assignment on an assigned request returns its existing object without changing the original admin or timestamps. A pending request whose `startsAt` has passed returns `409`. A missing request returns `404`.

Assignment and owner edits/deletions use conditional database writes. If assignment wins a race, the owner's edit/delete fails rather than overwriting it. Refresh the record after a conflict.

## Status and screen behaviour

| Status | Customer actions | Admin action |
| --- | --- | --- |
| `pending` | View, edit, delete | Mark assigned before hire start |
| `assigned` | View | View; repeated assignment returns the existing record |

A pending record stays pending after its start time passes; there is no automatic expired status. There is no unassign, completed, or cancelled status in this release.

Suggested customer flow:

1. Collect start/end times, pickup location, and optional notes.
2. Validate the future start, minimum duration, and text lengths before submitting.
3. Submit once and use the returned record to show “Pending assignment”.
4. Show requests from `GET /rentals`; allow editing and deletion for pending records.
5. Refresh detail on screen focus or an explicit refresh to observe assignment.
6. When assigned, show “Assigned” and remove edit/delete actions.

Suggested admin flow:

1. Load `GET /admin/rentals?status=pending`.
2. Open the request to review its dates, location, notes, and requesting user ID.
3. Arrange the car with the customer offline.
4. Call the assignment endpoint and replace the displayed record with its response.

There are no rental-specific WebSocket events or automatic notifications. Clients must refresh or poll to see changes. The request record does not supply the offline car arrangement details for display.

## Errors

Use the HTTP status to decide how to recover. Current application errors have more than one body format.

Validation example (`400`):

```json
{
  "message": "Bad request: Minimum hire duration is 24 hours",
  "status_code": 400
}
```

Assigned-record conflict (`409`):

```json
{
  "code": "scheduling_conflict",
  "message": "Assigned rentals cannot be changed or deleted"
}
```

The shared error handler currently uses `scheduling_conflict` for rental conflicts too. Assignment after the start time uses the same code with `"message": "Hire has already started"`.

Missing or inaccessible record (`404`):

```json
{
  "code": "not_found",
  "message": "Record not found"
}
```

| Status | Cause | Client handling |
| --- | --- | --- |
| `400` | Business validation, invalid query/UUID/JSON; missing token currently also returns 400 | Show a useful message; correct input or restore login |
| `401` | Invalid or expired access token | Use the existing authentication refresh/login flow |
| `403` | Non-admin caller uses an admin route | Hide admin actions and show access denied |
| `404` | Missing record or another user's record | Remove stale detail/list entry or show unavailable |
| `409` | Assigned record cannot be changed/deleted; pending hire has already started when assigning | Refresh the record and update available actions |
| `415` | JSON request lacks the required JSON content type | Send `Content-Type: application/json` |
| `422` | JSON fields have invalid types, required fields are missing, or unknown fields are supplied | Correct the request schema |
| `500` | Server/database failure | Show a retry option; avoid blindly retrying creation |

Framework extraction failures can return plain text instead of JSON. Do not assume every error response has a JSON body or `status_code`. Authentication is checked before rental business validation. For update requests, input validation runs before the ownership/status lookup, so invalid input may return 400 even when the target would otherwise return 404 or 409.

## TypeScript fetch example

```ts
export interface Rental {
  id: string;
  userId: string;
  startsAt: string;
  endsAt: string;
  pickupLocation: string;
  notes: string;
  status: "pending" | "assigned";
  assignedBy: string | null;
  assignedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface RentalInput {
  startsAt: string;
  endsAt: string;
  pickupLocation: string;
  notes?: string;
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

// apiBase includes /api/v1. Supply the latest token after login/refresh.
export function rentalClient(apiBase: string, accessToken: string) {
  const base = apiBase.replace(/\/$/, "");

  async function request<T>(path: string, method = "GET", body?: RentalInput): Promise<T> {
    const response = await fetch(`${base}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${accessToken}`,
        ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
      },
      ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
    });
    const text = await response.text();
    let data: unknown;
    try { data = text ? JSON.parse(text) : undefined; }
    catch { data = text; }

    if (!response.ok) {
      const message = typeof data === "object" && data !== null && "message" in data
        ? String(data.message)
        : typeof data === "string" && data ? data : `Request failed (${response.status})`;
      throw new ApiError(response.status, message);
    }
    return data as T; // undefined for 204; no JSON parsing on an empty body
  }

  function query(options: { status?: Rental["status"]; limit?: number; offset?: number } = {}) {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(options)) {
      if (value !== undefined) params.set(key, String(value));
    }
    return params.size ? `?${params.toString()}` : "";
  }

  return {
    create: (input: RentalInput) => request<Rental>("/rentals", "POST", input),
    list: (options?: Parameters<typeof query>[0]) => request<Rental[]>(`/rentals${query(options)}`),
    detail: (id: string) => request<Rental>(`/rentals/${encodeURIComponent(id)}`),
    update: (id: string, input: RentalInput) => request<Rental>(`/rentals/${encodeURIComponent(id)}`, "PUT", input),
    remove: (id: string) => request<void>(`/rentals/${encodeURIComponent(id)}`, "DELETE"),
    adminList: (options?: Parameters<typeof query>[0]) => request<Rental[]>(`/admin/rentals${query(options)}`),
    adminDetail: (id: string) => request<Rental>(`/admin/rentals/${encodeURIComponent(id)}`),
    assign: (id: string) => request<Rental>(`/admin/rentals/${encodeURIComponent(id)}/assign`, "POST"),
  };
}
```

Construct a new client when the access token changes. For cookie authentication, adapt the helper to use `credentials: "include"` and omit the bearer header.

## Backend rollout prerequisite

Apply the included rental migration before clients use these endpoints:

```sh
diesel migration run
```

The migration is `migrations/2026-11-01-000000_rental_requests`. Interactive API documentation is available at `/swagger-ui`, with the combined OpenAPI document at `/api-docs/openapi.json` on the API origin.
