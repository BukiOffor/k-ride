use crate::types::Pagination;

use super::*;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateUserDto {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub username: String,
    pub phone: String,
    pub address: String,
    pub country: String,
    pub city: Option<String>,
    pub password: String,
}

impl TryFrom<CreateUserDto> for User {
    type Error = ApiError;

    fn try_from(dto: CreateUserDto) -> Result<Self, Self::Error> {
        let password_hash = crate::helpers::password_hasher(&dto.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        Ok(User {
            id: Uuid::now_v7(),
            first_name: dto.first_name,
            last_name: dto.last_name,
            email: dto.email,
            username: dto.username,
            phone: dto.phone,
            address: dto.address,
            country: dto.country,
            city: dto.city.unwrap_or("N/A".to_string()),
            identity_type: None,
            identity_number: None,
            password_hash,
            is_identity_verified: false,
            is_active: true,
            role: RoleType::User,
            car: None,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            last_seen: chrono::Utc::now().naive_utc(),
        })
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AdminCreateUserDto {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub username: String,
    pub phone: String,
    pub address: String,
    pub city: Option<String>,
    pub country: String,
    pub password: String,
    pub is_active: Option<bool>,
    pub role: RoleType,
    pub car: Option<Uuid>,
}

impl TryFrom<AdminCreateUserDto> for User {
    type Error = ApiError;

    fn try_from(dto: AdminCreateUserDto) -> Result<Self, Self::Error> {
        let password_hash = crate::helpers::password_hasher(&dto.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        Ok(User {
            id: Uuid::now_v7(),
            first_name: dto.first_name,
            last_name: dto.last_name,
            email: dto.email,
            username: dto.username,
            phone: dto.phone,
            address: dto.address,
            country: dto.country,
            city: dto.city.unwrap_or("N/A".to_string()),
            identity_type: None,
            identity_number: None,
            password_hash,
            is_identity_verified: false,
            role: dto.role,
            is_active: dto.is_active.unwrap_or(true),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            last_seen: chrono::Utc::now().naive_utc(),
            car: dto.car,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AssignRoleDto {
    pub role: RoleType,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetUsersQuery {
    pub search: Option<String>,
    #[serde(alias = "roles")]
    pub role: Option<RoleType>,
    pub is_active: Option<bool>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::users)]
pub struct UpdateUserDto {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub identity_type: Option<String>,
    pub identity_number: Option<String>,
}

// A simple UserDto built from the model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub username: String,
    pub phone: String,
    pub address: String,
    pub city: String,
    pub country: String,
    pub identity_type: Option<String>,
    pub identity_number: Option<String>,
    pub is_identity_verified: bool,
    pub role: RoleType,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub last_seen: NaiveDateTime,
}

impl From<User> for UserDto {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            first_name: u.first_name,
            last_name: u.last_name,
            email: u.email,
            username: u.username,
            phone: u.phone,
            address: u.address,
            city: u.city,
            country: u.country,
            identity_type: u.identity_type,
            identity_number: u.identity_number,
            is_identity_verified: u.is_identity_verified,
            role: u.role,
            is_active: u.is_active,
            created_at: u.created_at,
            updated_at: u.updated_at,
            last_seen: u.last_seen,
        }
    }
}
