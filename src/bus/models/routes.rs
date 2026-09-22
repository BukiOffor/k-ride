use super::*;

#[derive(
    Debug,
    Clone,
    Queryable,
    Identifiable,
    Selectable,
    Serialize,
    Deserialize,
    Insertable,
    AsChangeset,
    utoipa::ToSchema,
)]
#[diesel(table_name = crate::schema::routes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Route {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub start_point: Uuid,
    pub end_point: Uuid,
    pub fare: f32,
    pub distance: f32,
    pub ride_type: RideType,
    pub stops: Vec<Uuid>,
    pub departure_time: chrono::NaiveTime,
    pub departure_date: chrono::NaiveDate,
    pub is_completed: bool,
    pub time_completed: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
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
pub enum RideType {
    Pickup,
    Dropoff,
    RoundTrip,
}

impl FromSql<Text, diesel::pg::Pg> for RideType {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Pickup" => Ok(RideType::Pickup),
            "Dropoff" => Ok(RideType::Dropoff),
            "RoundTrip" => Ok(RideType::RoundTrip),
            s => Err(format!("Unrecognized ride type: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for RideType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            RideType::Pickup => "Pickup",
            RideType::Dropoff => "Dropoff",
            RideType::RoundTrip => "RoundTrip",
        };
        std::io::Write::write_all(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}
