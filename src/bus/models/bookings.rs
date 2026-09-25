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
#[diesel(table_name = crate::schema::bookings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Booking {
    pub id: Uuid,
    pub booking_ref: String,
    pub user_id: Uuid,
    pub bus_id: Uuid,
    pub route_id: Uuid,
    pub seat_number: i32,
    pub status: BookingStatus,
    pub payment_status: PaymentStatus,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub qr_code: Option<String>,
    pub trip_id: Option<Uuid>,
}

impl Booking {
    pub fn new(user_id: Uuid, bus_id: Uuid, route_id: Uuid, trip_id: Option<Uuid>, seat_number: i32) -> Self {
        let now = chrono::Utc::now().naive_utc();
        let ref_code = Uuid::now_v7().to_string()[..8].to_uppercase();
        Self {
            id: Uuid::now_v7(),
            booking_ref: format!("BK-{}", ref_code),
            user_id,
            bus_id,
            route_id,
            trip_id,
            seat_number,
            status: BookingStatus::Confirmed,
            payment_status: PaymentStatus::Pending,
            created_at: now,
            updated_at: now,
            qr_code: None,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    AsExpression,
    FromSqlRow,
    PartialEq,
    Eq,
    Hash,
    utoipa::ToSchema,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum BookingStatus {
    Boarded,
    Confirmed,
    Cancelled,
    Revoked,
}

impl FromSql<Text, diesel::pg::Pg> for BookingStatus {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Boarded" => Ok(BookingStatus::Boarded),
            "Confirmed" => Ok(BookingStatus::Confirmed),
            "Cancelled" => Ok(BookingStatus::Cancelled),
            "Revoked" => Ok(BookingStatus::Revoked),
            s => Err(format!("Unrecognized booking status: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for BookingStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            BookingStatus::Boarded => "Boarded",
            BookingStatus::Confirmed => "Confirmed",
            BookingStatus::Cancelled => "Cancelled",
            BookingStatus::Revoked => "Revoked",
        };
        std::io::Write::write_all(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    AsExpression,
    FromSqlRow,
    PartialEq,
    Eq,
    Hash,
    utoipa::ToSchema,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum PaymentStatus {
    Free,
    Pending,
    Paid,
    Failed,
    Refunded,
}

impl FromSql<Text, diesel::pg::Pg> for PaymentStatus {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Free" => Ok(PaymentStatus::Free),
            "Pending" => Ok(PaymentStatus::Pending),
            "Paid" => Ok(PaymentStatus::Paid),
            "Failed" => Ok(PaymentStatus::Failed),
            "Refunded" => Ok(PaymentStatus::Refunded),
            s => Err(format!("Unrecognized payment status: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for PaymentStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            PaymentStatus::Free => "Free",
            PaymentStatus::Pending => "Pending",
            PaymentStatus::Paid => "Paid",
            PaymentStatus::Failed => "Failed",
            PaymentStatus::Refunded => "Refunded",
        };
        std::io::Write::write_all(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}
