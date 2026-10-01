use super::routes::RideType;
use super::*;

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
#[diesel(table_name = crate::schema::trips)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Trip {
    pub id: Uuid,
    pub route_id: Uuid,
    pub bus_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub departure_time: chrono::NaiveDateTime,
    pub arrival_time: chrono::NaiveDateTime,
    pub actual_departure_time: Option<chrono::NaiveDateTime>,
    pub actual_arrival_time: Option<chrono::NaiveDateTime>,
    pub ride_type: RideType,
    pub status: TripStatus,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    AsExpression,
    FromSqlRow,
    utoipa::ToSchema,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum TripStatus {
    Completed,
    Cancelled,
    InProgress,
    NotStarted,
}

impl FromSql<Text, diesel::pg::Pg> for TripStatus {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Completed" => Ok(TripStatus::Completed),
            "Cancelled" => Ok(TripStatus::Cancelled),
            "InProgress" => Ok(TripStatus::InProgress),
            "NotStarted" => Ok(TripStatus::NotStarted),
            s => Err(format!("Unrecognized trip status: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for TripStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            TripStatus::Completed => "Completed",
            TripStatus::Cancelled => "Cancelled",
            TripStatus::InProgress => "InProgress",
            TripStatus::NotStarted => "NotStarted",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}
