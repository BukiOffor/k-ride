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
#[diesel(table_name = crate::schema::cars)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Car {
    pub id: Uuid,
    #[serde(alias = "plate", alias = "plate_number")]
    pub license_plate: String,
    #[serde(alias = "passenger_seats", alias = "seats")]
    pub capacity: i32,
    pub car_type: CarType,
    pub car_tier: CarTier,
    pub car_size: CarSize,
    #[serde(alias = "colour")]
    pub color: String,
    pub color_hex: Option<String>,
    #[serde(alias = "make")]
    pub brand: String,
    pub model: String,
    pub year: i32,
    pub vehicle_photo: Option<String>,
    pub vehicle_registration: Option<String>,
    pub insurance_certificate: Option<String>,
    pub roadworthiness_certificate: Option<String>,
    pub is_verified: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

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
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum CarType {
    Saloon,
    SUV,
    Van,
}

impl FromSql<Text, diesel::pg::Pg> for CarType {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Saloon" => Ok(Self::Saloon),
            "SUV" => Ok(Self::SUV),
            "Van" => Ok(Self::Van),
            s => Err(format!("Unrecognized car type: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for CarType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            Self::Saloon => "Saloon",
            Self::SUV => "SUV",
            Self::Van => "Van",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

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
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum CarTier {
    Comfort,
    Premium,
    Luxury,
}

impl FromSql<Text, diesel::pg::Pg> for CarTier {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Comfort" => Ok(Self::Comfort),
            "Premium" => Ok(Self::Premium),
            "Luxury" => Ok(Self::Luxury),
            s => Err(format!("Unrecognized car tier: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for CarTier {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            Self::Comfort => "Comfort",
            Self::Premium => "Premium",
            Self::Luxury => "Luxury",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

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
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum CarSize {
    // signifies salon cars that can take 1-2 passengers
    Standard,
    // signifies SUVs that can take 1-4 passengers
    Large,
    // signifies cars that can take 1-7 passengers
    XtraLarge,
}

impl FromSql<Text, diesel::pg::Pg> for CarSize {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Standard" => Ok(Self::Standard),
            "Large" => Ok(Self::Large),
            "XtraLarge" => Ok(Self::XtraLarge),
            s => Err(format!("Unrecognized car size: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for CarSize {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            Self::Standard => "Standard",
            Self::Large => "Large",
            Self::XtraLarge => "XtraLarge",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}
