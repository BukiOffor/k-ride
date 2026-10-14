use super::*;
use chrono::{NaiveDate, NaiveDateTime};

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
#[diesel(table_name = crate::schema::driver_profiles)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct DriverProfile {
    pub id: Uuid,
    pub user_id: Uuid,
    pub car_id: Uuid,
    pub nin_slip: String,
    pub profile_photo: String,
    pub is_koinonia_worker: bool,
    pub koinonia_department: Option<String>,
    pub guarantor_name: Option<String>,
    pub guarantor_phone: Option<String>,
    pub guarantor_relationship: Option<String>,
    pub guarantor_photo: Option<String>,
    pub licence_number: String,
    pub licence_class: String,
    pub licence_expiry_date: NaiveDate,
    pub licence_photo: String,
    pub track: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
