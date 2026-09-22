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
#[diesel(table_name = crate::schema::points)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Point {
    pub id: Uuid,
    pub name: String,
    pub geo_location: GeoPoint,
    pub landmark: String,
    pub description: String,
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
    utoipa::ToSchema,
)]
#[diesel(sql_type = diesel::sql_types::Jsonb)]
pub struct GeoPoint {
    pub lat: String,
    pub long: String,
}

impl FromSql<diesel::sql_types::Jsonb, diesel::pg::Pg> for GeoPoint {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        let bytes = bytes.as_bytes();
        if bytes.is_empty() {
            return Err("Empty jsonb value".into());
        }
        let json_bytes = if bytes[0] == 1 {
            &bytes[1..]
        } else {
            bytes
        };
        serde_json::from_slice(json_bytes).map_err(Into::into)
    }
}

impl ToSql<diesel::sql_types::Jsonb, diesel::pg::Pg> for GeoPoint {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        std::io::Write::write_all(&mut *out, &[1])?;
        serde_json::to_writer(&mut *out, self)?;
        Ok(serialize::IsNull::No)
    }
}