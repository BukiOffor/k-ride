use super::*;

#[derive(
    Debug,
    Clone,
    Queryable,
    Identifiable,
    Serialize,
    Selectable,
    Deserialize,
    Insertable,
    AsChangeset,
    utoipa::ToSchema,
)]
#[diesel(table_name = schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub username: String,
    pub phone: String,
    pub address: String,
    pub country: String,
    pub identity_type: Option<String>,
    pub identity_number: Option<String>,
    pub password_hash: String,
    pub is_identity_verified: bool,
    pub role: RoleType,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub last_seen: NaiveDateTime,
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
pub enum RoleType {
    Root,
    Admin,
    RouteCoordinator,
    User,
    Driver,
}

impl FromSql<Text, diesel::pg::Pg> for RoleType {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        match std::str::from_utf8(bytes.as_bytes())? {
            "Admin" => Ok(RoleType::Admin),
            "RouteCoordinator" => Ok(RoleType::RouteCoordinator),
            "Driver" => Ok(RoleType::Driver),
            "Root" => Ok(RoleType::Root),
            "User" => Ok(RoleType::User),
            s => Err(format!("Unrecognized role type: {}", s).into()),
        }
    }
}

impl ToSql<Text, diesel::pg::Pg> for RoleType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = match self {
            RoleType::Admin => "Admin",
            RoleType::RouteCoordinator => "RouteCoordinator",
            RoleType::Driver => "Driver",
            RoleType::Root => "Root",
            RoleType::User => "User",
        };
        std::io::Write::write(&mut *out, s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}