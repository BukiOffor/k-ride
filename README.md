# K-Ride Backend (`k-rides`)

> **Project Status — MVP:** This repository contains the **Minimum Viable Product (MVP)** backend for the **K-Ride** mobility and transportation platform. Core flows—including on-demand private rides, scheduled rides, bus/shuttle bookings, and multi-day car rentals—are implemented. **Ride sharing (pooled multi-passenger rides) is not yet implemented and is planned for an upcoming release.**

---

## Overview

**K-Ride** is a unified mobility backend built in Rust with Axum, Diesel (async PostgreSQL), Redis, and H3 spatial indexing. It powers urban and community transportation across four main verticals:

1. **On-Demand Private Rides**: Real-time fare estimation over OSRM road routes, proximity-based driver dispatch using Uber H3 cells and Redis, live WebSocket ride events, 4-digit pickup PIN verification, waiting-time billing, ratings, and SOS safety reporting.
2. **Scheduled Rides**: Advance private ride bookings (1 hour to 30 days ahead) with timezone validation, conflict/overlap prevention, a driver reservation board, and background workers for reminders and activation.
3. **Bus & Shuttle Transit**: Route and stop management, bus fleet tracking, scheduled trips, seat bookings with QR code tickets, and passenger route demand requests.
4. **Car Rentals**: Offline-assisted multi-day vehicle hire requests (`Saloon`, `SUV`, `Van`, 24-hour minimum duration) with daily pricing and administrator driver assignment.

---

## Current MVP Features vs. Roadmap

### Included in the MVP
- **Authentication & RBAC**: Argon2 password hashing, JWT (Bearer & HTTP-only cookie) authentication, password reset flows, and role-based access control (`User`, `Driver`, `RouteCoordinator`, `Admin`, `Root`).
- **Driver & Vehicle Onboarding**: Vehicle registration (`Saloon`, `SUV`, `Van`), document tracking, and driver profile verification.
- **Road-Validated Fare Engine**: Dynamic pricing based on distance, duration, base fare, minimum fare, waiting policy, and fixed-route rules (e.g., Nnamdi Azikiwe International Airport fixed fare).
- **Spatial Dispatch Engine (`dispatcher` crate)**: H3 geospatial indexing backed by Redis and OSRM road-ETA ranking to match nearby available drivers.
- **Real-Time WebSockets**: Live ride status transitions, driver location streaming, and offer notifications over `/rides`.
- **Scheduled Ride & Email Outbox Workers**: Durable PostgreSQL-backed outbox queues for transactional emails (Lettre + Handlebars) and scheduled ride lifecycle events.
- **Admin Operations & Audit Logs**: Administrative management for users, drivers, rides, safety incidents, rental assignments, and asynchronous activity logging.

### Coming Soon (Post-MVP Roadmap)
- **Ride Sharing (Pooled Rides)**: Matching multiple passengers heading in the same direction into a single vehicle, dynamic detour/waypoint optimization, capacity-aware dispatch, and shared fare splitting.
- **Online Payment Gateway Integration**: Automated card/wallet settlement beyond cash and offline fulfillment.
- **Self-Service Rental Fleet Calendars**: Automated vehicle availability calendars and organization-managed rental fleets.

---

## Tech Stack

- **Web Framework**: [Axum](https://github.com/tokio-rs/axum) (REST API + WebSockets)
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **Database & ORM**: PostgreSQL with [Diesel](https://diesel.rs/), [diesel-async](https://github.com/weiznich/diesel-async), and [bb8](https://github.com/djc/bb8) connection pooling
- **Geospatial & Caching**: Redis + [h3o](https://github.com/HydroniumLabs/h3o) (H3 spatial index) + OSRM routing (`dispatcher` workspace crate)
- **Authentication**: `jsonwebtoken` (JWT) + `argon2`
- **Email Delivery**: [Lettre](https://github.com/lettre/lettre) with [Handlebars](https://github.com/sunng87/handlebars-rust) templates
- **API Documentation**: [Utoipa](https://github.com/juhakivekas/utoipa) OpenAPI 3 + Swagger UI
- **Tickets & Media**: `qrcode` and `image` for booking QR codes

---

## Project Structure

```text
.
├── dispatcher/            # H3 geospatial driver indexing & OSRM road-routing crate
├── string_validator/      # Input validation helper crate
├── migrations/            # Diesel PostgreSQL schema migrations
└── src/
    ├── admin/             # Admin management, incident handling, and oversight endpoints
    ├── auth/              # Login, registration, JWT claims, and password recovery
    ├── bus/               # Bus routes, points, trips, seat bookings, and QR codes
    ├── helpers/           # Fare calculator, hashing, and shared utilities
    ├── logs/              # Asynchronous activity logging workers and endpoints
    ├── mailer/            # Transactional email templates and durable outbox worker
    ├── rentals.rs         # Multi-day car rental requests and admin driver assignment
    ├── users/             # User accounts, roles, and driver profile onboarding
    ├── vrp/               # On-demand rides, scheduled rides, WebSocket realtime hub
    ├── config.rs          # Database pool and worker initialization
    ├── errors.rs          # Centralized error types and HTTP status mapping
    ├── schema.rs          # Diesel database schema definitions
    ├── lib.rs             # Shared application state and module exports
    └── main.rs            # Server entrypoint, middleware, and route mounting
```

---

## Getting Started

### Prerequisites

- **Rust** (2024 edition)
- **PostgreSQL**
- **Redis**
- **Diesel CLI** (`cargo install diesel_cli --no-default-features --features postgres`)

### 1. Environment Configuration

Create a `.env` file in the project root:

```env
DATABASE_URL=postgres://username:password@localhost:5432/k_ride
REDIS_URL=redis://127.0.0.1:6379/
JWT_SECRET=your_jwt_secret_key
SMTP_HOST=smtp.example.com
SMTP_PORT=587
SMTP_USER=user@example.com
SMTP_PASS=password
```

### 2. Run Database Migrations

Pending migrations run automatically when the server starts, or you can apply them manually with Diesel CLI:

```bash
diesel migration run
```

### 3. Run the Server

```bash
cargo run
```

The server listens on `0.0.0.0:9990`:
- **REST API Base URL**: `http://localhost:9990/api/v1`
- **WebSocket Endpoint**: `ws://localhost:9990/rides`

---

## API Documentation

Interactive OpenAPI documentation is generated automatically via Utoipa:

- **Swagger UI**: `http://localhost:9990/swagger-ui`
- **OpenAPI JSON**: `http://localhost:9990/api-docs/openapi.json`

---

## Running Tests

```bash
cargo test --lib
```
