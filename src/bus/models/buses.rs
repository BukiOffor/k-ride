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
#[diesel(table_name = crate::schema::buses)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Bus {
    pub id: Uuid,
    pub license_plate: String,
    pub capacity: i32,
    pub current_passenger_count: i32,
    pub current_trip_id: Option<Uuid>,
    pub state: BusState,
    pub status: BusStatus,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
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
pub enum BusStatus {
    Available,
    Maintenance,
    InUse,
}

impl FromSql<Text, diesel::pg::Pg> for BusStatus {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Available" => Ok(BusStatus::Available),
            "Maintenance" => Ok(BusStatus::Maintenance),
            "InUse" => Ok(BusStatus::InUse),
            s => Err(format!("Unrecognized bus status: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for BusStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            BusStatus::Available => "Available",
            BusStatus::Maintenance => "Maintenance",
            BusStatus::InUse => "InUse",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
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
pub enum BusState {
    Transit,
    Arrived,
    Stationary,
    Boarding,
}

impl FromSql<Text, diesel::pg::Pg> for BusState {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Transit" => Ok(Self::Transit),
            "Arrived" => Ok(Self::Arrived),
            "Stationary" => Ok(Self::Stationary),
            "Boarding" => Ok(Self::Boarding),
            s => Err(format!("Unrecognized bus status: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for BusState {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            Self::Transit => "Transit",
            Self::Stationary => "Stationary",
            Self::Boarding => "Boarding",
            Self::Arrived => "Arrived",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}
