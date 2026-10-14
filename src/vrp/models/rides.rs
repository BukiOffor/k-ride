//! Persistence models for Paid Rides.
//!
//! Complex value objects (places, routes, fare quotes, tracking) are stored as
//! JSONB and (de)serialised into the DTO types in `dtos::rides` by the services.

use super::*;
use chrono::NaiveDateTime;
use std::str::FromStr;

/// Ride lifecycle, see §7 of `PAID_RIDES_BACKEND_REQUIREMENTS.md`.
#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    AsExpression,
    FromSqlRow,
    PartialEq,
    Eq,
    Hash,
    utoipa::ToSchema,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::AsRefStr,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum RideStatus {
    Requested,
    Searching,
    NoDrivers,
    DriverAssigned,
    DriverEnRoute,
    DriverArriving,
    DriverArrived,
    InProgress,
    Paused,
    Completed,
    Cancelled,
}

impl RideStatus {
    /// Statuses that still need attention (not terminal).
    pub const LIVE: [RideStatus; 9] = [
        RideStatus::Requested,
        RideStatus::Searching,
        RideStatus::NoDrivers,
        RideStatus::DriverAssigned,
        RideStatus::DriverEnRoute,
        RideStatus::DriverArriving,
        RideStatus::DriverArrived,
        RideStatus::InProgress,
        RideStatus::Paused,
    ];

    /// Statuses during which a driver is attached and heading to / with the rider.
    pub const DRIVER_ATTACHED: [RideStatus; 6] = [
        RideStatus::DriverAssigned,
        RideStatus::DriverEnRoute,
        RideStatus::DriverArriving,
        RideStatus::DriverArrived,
        RideStatus::InProgress,
        RideStatus::Paused,
    ];

    pub fn is_terminal(self) -> bool {
        matches!(self, RideStatus::Completed | RideStatus::Cancelled)
    }

    /// The transition table from §7. Anything not listed is rejected with 409.
    pub fn can_transition_to(self, next: RideStatus) -> bool {
        use RideStatus::*;
        match self {
            Requested => matches!(next, Searching | NoDrivers | Cancelled),
            Searching => matches!(next, DriverAssigned | NoDrivers | Cancelled),
            NoDrivers => matches!(next, Searching | Cancelled),
            DriverAssigned => matches!(
                next,
                DriverEnRoute | DriverArriving | DriverArrived | Searching | Cancelled
            ),
            DriverEnRoute => matches!(next, DriverArriving | DriverArrived | Searching | Cancelled),
            DriverArriving => {
                matches!(next, DriverEnRoute | DriverArrived | Searching | Cancelled)
            }
            DriverArrived => matches!(next, InProgress | Cancelled),
            InProgress => matches!(next, Paused | Completed | Cancelled),
            Paused => matches!(next, InProgress | Completed | Cancelled),
            Completed | Cancelled => false,
        }
    }

    pub fn live_strings() -> Vec<String> {
        Self::LIVE.iter().map(|s| s.to_string()).collect()
    }
}

impl FromSql<Text, diesel::pg::Pg> for RideStatus {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        RideStatus::from_str(s).map_err(|_| format!("Unrecognized ride status: {}", s).into())
    }
}

impl ToSql<Text, diesel::pg::Pg> for RideStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        std::io::Write::write(&mut *out, self.as_ref().as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

/// Driver live state (§1 DriverAvailability).
#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    utoipa::ToSchema,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::AsRefStr,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AvailabilityStatus {
    Offline,
    Online,
    OnTrip,
}

/// Offer lifecycle (§1 RideRequest / Offer).
#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    utoipa::ToSchema,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum OfferStatus {
    Offered,
    Accepted,
    Declined,
    Expired,
    Withdrawn,
}

/// A ride product with all the numbers the fare estimator needs.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Queryable,
    Identifiable,
    Selectable,
    Insertable,
    AsChangeset,
    utoipa::ToSchema,
)]
#[diesel(table_name = crate::schema::ride_options)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RideOption {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `compact | sedan | suv | van | premium` shown to riders.
    pub vehicle_class: String,
    /// Driver vehicle classes allowed to serve this product.
    pub eligible_vehicle_classes: Vec<String>,
    pub capacity: i32,
    pub shared: bool,
    pub currency: String,
    pub base_fare_minor: i64,
    pub per_km_minor: i64,
    pub per_minute_minor: i64,
    pub minimum_fare_minor: i64,
    pub booking_fee_minor: i64,
    /// Free waiting at pickup once the driver has arrived (default 5).
    pub free_waiting_minutes: i32,
    /// Charged for every started minute after the free waiting window.
    pub waiting_per_minute_minor: i64,
    /// Totals are rounded up to a multiple of this (e.g. 5000 = ₦50). 0 disables.
    pub rounding_minor: i64,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::fare_estimates)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct FareEstimateRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub pickup: serde_json::Value,
    pub destination: serde_json::Value,
    pub route: serde_json::Value,
    pub options: serde_json::Value,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Identifiable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::rides)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(treat_none_as_null = true)]
pub struct Ride {
    pub id: Uuid,
    pub passenger_id: Uuid,
    pub driver_id: Option<Uuid>,
    pub status: RideStatus,
    pub version: i32,
    pub option_id: String,
    pub option_snapshot: serde_json::Value,
    pub estimate_id: Uuid,
    pub pickup: serde_json::Value,
    pub destination: serde_json::Value,
    pub route: serde_json::Value,
    pub fare: serde_json::Value,
    pub currency: String,
    pub fare_total_minor: i64,
    pub waiting_charge_minor: i64,
    pub final_fare_minor: Option<i64>,
    pub payment_method: String,
    pub payment_status: String,
    pub trip_pin: String,
    pub dispatch_round: i32,
    pub cancellation: Option<serde_json::Value>,
    pub tracking: Option<serde_json::Value>,
    pub rating: Option<serde_json::Value>,
    pub sos_status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub assigned_at: Option<NaiveDateTime>,
    pub arrived_at: Option<NaiveDateTime>,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub cancelled_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Queryable, Identifiable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::ride_offers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RideOfferRecord {
    pub id: Uuid,
    pub ride_id: Uuid,
    pub driver_id: Uuid,
    pub dispatch_round: i32,
    pub status: String,
    pub pickup_distance_m: Option<i32>,
    pub pickup_eta_seconds: Option<i32>,
    pub expires_at: NaiveDateTime,
    pub responded_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::driver_availability)]
#[diesel(primary_key(driver_id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct DriverAvailabilityRecord {
    pub driver_id: Uuid,
    pub status: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub heading: Option<f64>,
    pub speed_mps: Option<f64>,
    pub accuracy_m: Option<f64>,
    pub recorded_at: Option<NaiveDateTime>,
    pub received_at: Option<NaiveDateTime>,
    pub updated_at: NaiveDateTime,
}

impl DriverAvailabilityRecord {
    pub fn status(&self) -> AvailabilityStatus {
        AvailabilityStatus::from_str(&self.status).unwrap_or(AvailabilityStatus::Offline)
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::ride_ratings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RideRatingRecord {
    pub id: Uuid,
    pub ride_id: Uuid,
    pub rater_id: Uuid,
    pub ratee_id: Uuid,
    pub rater_role: String,
    pub stars: i16,
    pub tags: Vec<String>,
    pub comment: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::safety_events)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct SafetyEventRecord {
    pub id: Uuid,
    pub ride_id: Uuid,
    pub reporter_id: Uuid,
    pub reporter_role: String,
    pub kind: String,
    pub event_type: String,
    pub details: Option<String>,
    pub location: Option<serde_json::Value>,
    pub driver_location: Option<serde_json::Value>,
    pub status: String,
    pub occurred_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::idempotency_keys)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct IdempotencyRecord {
    pub user_id: Uuid,
    pub endpoint: String,
    pub idempotency_key: String,
    pub status_code: i32,
    pub response: serde_json::Value,
    pub created_at: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_as_screaming_snake() {
        assert_eq!(RideStatus::DriverEnRoute.to_string(), "DRIVER_EN_ROUTE");
        assert_eq!(
            RideStatus::from_str("NO_DRIVERS").unwrap(),
            RideStatus::NoDrivers
        );
        assert_eq!(
            serde_json::to_string(&RideStatus::InProgress).unwrap(),
            "\"IN_PROGRESS\""
        );
    }

    #[test]
    fn transition_table_matches_spec() {
        use RideStatus::*;
        assert!(Requested.can_transition_to(Searching));
        assert!(Searching.can_transition_to(DriverAssigned));
        assert!(DriverEnRoute.can_transition_to(Searching));
        assert!(DriverArrived.can_transition_to(InProgress));
        assert!(!DriverArrived.can_transition_to(Searching));
        assert!(!InProgress.can_transition_to(DriverArrived));
        assert!(!Completed.can_transition_to(Cancelled));
        assert!(!Cancelled.can_transition_to(Searching));
        assert!(NoDrivers.can_transition_to(Searching));
    }
}
