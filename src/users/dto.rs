use crate::types::Pagination;
pub use crate::vrp::dtos::drivers::{
    DriverProfileDto, DriverRegistrationDto, DriverTrack, LicenceClass, PassengerSeats,
};

use super::*;

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
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
    pub account_number: Option<String>,
    pub bank_name: Option<String>,
    #[serde(default)]
    pub role: Option<RoleType>,
    #[serde(alias = "driver_profile", alias = "driver")]
    pub driver_details: Option<DriverRegistrationDto>,

}

impl TryFrom<CreateUserDto> for User {
    type Error = ApiError;

    fn try_from(dto: CreateUserDto) -> Result<Self, Self::Error> {
        let password_hash = crate::helpers::password_hasher(&dto.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        let is_driver = dto.role == Some(RoleType::Driver) || dto.driver_details.is_some();
        let (role, identity_type, identity_number) = if is_driver {
            let nin = dto.driver_details.as_ref().map(|d| d.nin.trim().to_string());
            (RoleType::Driver, Some("NIN".to_string()), nin)
        } else {
            (dto.role.unwrap_or(RoleType::User), None, None)
        };

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
            identity_type,
            identity_number,
            password_hash,
            is_identity_verified: false,
            is_active: true,
            role,
            car: None,
            account_number: dto.account_number,
            bank_name: dto.bank_name,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            last_seen: chrono::Utc::now().naive_utc(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
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
    #[serde(alias = "driver_profile", alias = "driver")]
    pub driver_details: Option<DriverRegistrationDto>,
    pub account_number: Option<String>,
    pub bank_name: Option<String>,
}

impl TryFrom<AdminCreateUserDto> for User {
    type Error = ApiError;

    fn try_from(dto: AdminCreateUserDto) -> Result<Self, Self::Error> {
        let password_hash = crate::helpers::password_hasher(&dto.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        let (identity_type, identity_number) = if let Some(ref d) = dto.driver_details {
            (Some("NIN".to_string()), Some(d.nin.trim().to_string()))
        } else {
            (None, None)
        };

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
            identity_type,
            identity_number,
            password_hash,
            is_identity_verified: false,
            role: dto.role,
            is_active: dto.is_active.unwrap_or(true),
            account_number: dto.account_number,
            bank_name: dto.bank_name,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_profile: Option<DriverProfileDto>,
}

impl UserDto {
    pub fn with_driver_profile(mut self, profile: DriverProfileDto) -> Self {
        self.driver_profile = Some(profile);
        self
    }
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
            car: u.car,
            driver_profile: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regular_create_user_dto() {
        let json_str = r#"{
            "first_name": "John",
            "last_name": "Doe",
            "email": "john@example.com",
            "username": "johndoe",
            "phone": "+2348012345678",
            "address": "123 Street",
            "country": "Nigeria",
            "city": "Abuja",
            "password": "Password123"
        }"#;

        let dto: CreateUserDto = serde_json::from_str(json_str).unwrap();
        assert_eq!(dto.first_name, "John");
        assert!(dto.driver_details.is_none());

        let user: User = dto.try_into().unwrap();
        assert_eq!(user.role, RoleType::User);
        assert_eq!(user.identity_type, None);
        assert_eq!(user.identity_number, None);
    }

    #[test]
    fn test_driver_create_user_dto() {
        let json_str = r#"{
            "first_name": "Adebayo",
            "last_name": "Ogunlesi",
            "email": "adebayo@example.com",
            "username": "adebayo",
            "phone": "+2348099998888",
            "address": "456 Avenue",
            "country": "Nigeria",
            "city": "Abuja",
            "password": "Password123",
            "driver_details": {
                "nin": "12345678901",
                "nin_slip": "data:image/png;base64,slip",
                "profile_photo": "data:image/png;base64,face",
                "is_koinonia_worker": true,
                "koinonia_department": "Ushering",
                "licence_number": "KD-998877",
                "licence_class": "B",
                "licence_expiry_date": "2028-06-30",
                "licence_photo": "data:image/png;base64,licence",
                "vehicle_make": "Toyota",
                "vehicle_model": "Camry",
                "vehicle_colour": "Blue",
                "vehicle_year": 2019,
                "vehicle_plate_number": "ABC-123-XY",
                "passenger_seats": 4,
                "vehicle_registration": "data:image/png;base64,reg",
                "insurance_certificate": "data:image/png;base64,ins",
                "roadworthiness_certificate": "data:image/png;base64,road",
                "track": "Professional"
            }
        }"#;

        let dto: CreateUserDto = serde_json::from_str(json_str).unwrap();
        assert!(dto.driver_details.is_some());

        let driver_details = dto.driver_details.as_ref().unwrap();
        assert_eq!(driver_details.nin, "12345678901");
        assert!(driver_details.validate().is_ok());

        let user: User = dto.try_into().unwrap();
        assert_eq!(user.role, RoleType::Driver);
        assert_eq!(user.identity_type, Some("NIN".to_string()));
        assert_eq!(user.identity_number, Some("12345678901".to_string()));
    }
}

