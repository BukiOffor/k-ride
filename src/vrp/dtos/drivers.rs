use super::super::models::drivers::DriverProfile;
use super::*;
use chrono::{Datelike, NaiveDate, NaiveDateTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum LicenceClass {
    #[serde(alias = "b", alias = "B")]
    B,
    #[serde(alias = "c", alias = "C")]
    C,
    #[serde(alias = "d", alias = "D")]
    D,
    #[serde(alias = "e", alias = "E")]
    E,
}

impl std::fmt::Display for LicenceClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenceClass::B => write!(f, "B"),
            LicenceClass::C => write!(f, "C"),
            LicenceClass::D => write!(f, "D"),
            LicenceClass::E => write!(f, "E"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum DriverTrack {
    #[serde(alias = "volunteer", alias = "VOLUNTEER")]
    Volunteer,
    #[serde(alias = "professional", alias = "PROFESSIONAL")]
    Professional,
    #[serde(alias = "both", alias = "BOTH")]
    Both,
}

impl std::fmt::Display for DriverTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverTrack::Volunteer => write!(f, "Volunteer"),
            DriverTrack::Professional => write!(f, "Professional"),
            DriverTrack::Both => write!(f, "Both"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct PassengerSeats(pub i32);

impl<'de> Deserialize<'de> for PassengerSeats {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PassengerSeatsVisitor;
        impl<'de> serde::de::Visitor<'de> for PassengerSeatsVisitor {
            type Value = PassengerSeats;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "an integer (e.g. 3, 4, 5, 6) or string representation ('3', '4', '5', '6+')",
                )
            }

            fn visit_i64<E>(self, v: i64) -> Result<PassengerSeats, E>
            where
                E: serde::de::Error,
            {
                Ok(PassengerSeats(v as i32))
            }

            fn visit_u64<E>(self, v: u64) -> Result<PassengerSeats, E>
            where
                E: serde::de::Error,
            {
                Ok(PassengerSeats(v as i32))
            }

            fn visit_str<E>(self, v: &str) -> Result<PassengerSeats, E>
            where
                E: serde::de::Error,
            {
                let trimmed = v.trim().trim_end_matches('+');
                trimmed
                    .parse::<i32>()
                    .map(PassengerSeats)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(PassengerSeatsVisitor)
    }
}

impl From<i32> for PassengerSeats {
    fn from(val: i32) -> Self {
        PassengerSeats(val)
    }
}

impl From<PassengerSeats> for i32 {
    fn from(seats: PassengerSeats) -> Self {
        seats.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DriverRegistrationDto {
    // Step 1: Identity
    pub nin: String,
    pub nin_slip: String,
    pub profile_photo: String,
    #[serde(default, alias = "koinonia_worker")]
    pub is_koinonia_worker: bool,
    pub koinonia_department: Option<String>,
    pub guarantor_name: Option<String>,
    pub guarantor_phone: Option<String>,
    pub guarantor_relationship: Option<String>,
    pub guarantor_photo: Option<String>,

    // Step 2: Licence
    #[serde(alias = "license_number")]
    pub licence_number: String,
    #[serde(alias = "license_class")]
    pub licence_class: LicenceClass,
    #[serde(alias = "license_expiry_date")]
    pub licence_expiry_date: NaiveDate,
    #[serde(
        alias = "license_photo",
        alias = "licence_scan",
        alias = "license_scan"
    )]
    pub licence_photo: String,

    // Step 3: Vehicle
    #[serde(alias = "make", alias = "brand")]
    pub vehicle_make: String,
    #[serde(alias = "model")]
    pub vehicle_model: String,
    #[serde(alias = "colour", alias = "color")]
    pub vehicle_colour: String,
    #[serde(alias = "year")]
    pub vehicle_year: i32,
    #[serde(alias = "plate_number", alias = "license_plate")]
    pub vehicle_plate_number: String,
    #[serde(
        alias = "seats",
        alias = "capacity",
        alias = "number_of_passenger_seats"
    )]
    pub passenger_seats: PassengerSeats,
    #[serde(alias = "car_photo")]
    pub vehicle_photo: Option<String>,

    // Step 4: Vehicle documents
    #[serde(alias = "proof_of_ownership", alias = "registration_document")]
    pub vehicle_registration: String,
    #[serde(alias = "insurance", alias = "insurance_document")]
    pub insurance_certificate: String,
    #[serde(alias = "roadworthiness", alias = "roadworthiness_document")]
    pub roadworthiness_certificate: String,

    // Step 5: Track
    pub track: DriverTrack,
}

impl DriverRegistrationDto {
    pub fn validate(&self) -> Result<(), ApiError> {
        // Step 1: Identity validation
        let nin = self.nin.trim();
        if nin.len() != 11 || !nin.chars().all(|c| c.is_ascii_digit()) {
            return Err(ApiError::BadRequest("11-digit NIN is required"));
        }

        if self.nin_slip.trim().is_empty() {
            return Err(ApiError::BadRequest("NIN slip is required"));
        }

        if self.profile_photo.trim().is_empty() {
            return Err(ApiError::BadRequest("Profile photo of face is required"));
        }

        if self.is_koinonia_worker {
            match &self.koinonia_department {
                Some(dept) if !dept.trim().is_empty() => (),
                _ => {
                    return Err(ApiError::BadRequest(
                        "Koinonia department is required when user is a Koinonia worker",
                    ));
                }
            }
        } else {
            match &self.guarantor_name {
                Some(name) if !name.trim().is_empty() => (),
                _ => return Err(ApiError::BadRequest("Guarantor's full name is required")),
            }
            match &self.guarantor_phone {
                Some(phone) if !phone.trim().is_empty() => (),
                _ => return Err(ApiError::BadRequest("Guarantor's phone number is required")),
            }
            match &self.guarantor_relationship {
                Some(rel) if !rel.trim().is_empty() => (),
                _ => return Err(ApiError::BadRequest("Guarantor's relationship is required")),
            }
            match &self.guarantor_photo {
                Some(photo) if !photo.trim().is_empty() => (),
                _ => return Err(ApiError::BadRequest("Guarantor's photo is required")),
            }
        }

        // Step 2: Licence validation
        if self.licence_number.trim().is_empty() {
            return Err(ApiError::BadRequest("Licence number is required"));
        }

        let today = chrono::Utc::now().date_naive();
        let min_expiry = today
            .checked_add_months(chrono::Months::new(3))
            .unwrap_or_else(|| today + chrono::Duration::days(90));
        if self.licence_expiry_date < min_expiry {
            return Err(ApiError::BadRequest(
                "Driver's licence must be valid for at least 3 more months",
            ));
        }

        if self.licence_photo.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Driver's licence photo or scan is required",
            ));
        }

        // Step 3: Vehicle validation
        if self.vehicle_make.trim().is_empty() {
            return Err(ApiError::BadRequest("Vehicle make is required"));
        }
        if self.vehicle_model.trim().is_empty() {
            return Err(ApiError::BadRequest("Vehicle model is required"));
        }
        if self.vehicle_colour.trim().is_empty() {
            return Err(ApiError::BadRequest("Vehicle colour is required"));
        }

        let current_year = chrono::Utc::now().year();
        let max_year = current_year + 1;
        if self.vehicle_year < 1990 || self.vehicle_year > max_year {
            return Err(ApiError::BadRequest(
                "Vehicle year must be between 1990 and next year",
            ));
        }

        let plate = self.vehicle_plate_number.trim().to_uppercase();
        let parts: Vec<&str> = plate.split('-').collect();
        let valid_plate = parts.len() == 3
            && parts[0].len() == 3
            && parts[0].chars().all(|c| c.is_ascii_alphabetic())
            && parts[1].len() == 3
            && parts[1].chars().all(|c| c.is_ascii_digit())
            && parts[2].len() == 2
            && parts[2].chars().all(|c| c.is_ascii_alphabetic());
        if !valid_plate {
            return Err(ApiError::BadRequest(
                "Vehicle plate number must follow format ABC-123-XY",
            ));
        }

        if self.passenger_seats.0 < 3 {
            return Err(ApiError::BadRequest(
                "Number of passenger seats must be 3, 4, 5, or 6+",
            ));
        }

        // Step 4: Vehicle documents validation
        if self.vehicle_registration.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Vehicle registration document is required",
            ));
        }
        if self.insurance_certificate.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Insurance certificate document is required",
            ));
        }
        if self.roadworthiness_certificate.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Roadworthiness certificate document is required",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DriverProfileDto {
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

impl From<DriverProfile> for DriverProfileDto {
    fn from(p: DriverProfile) -> Self {
        Self {
            id: p.id,
            user_id: p.user_id,
            car_id: p.car_id,
            nin_slip: p.nin_slip,
            profile_photo: p.profile_photo,
            is_koinonia_worker: p.is_koinonia_worker,
            koinonia_department: p.koinonia_department,
            guarantor_name: p.guarantor_name,
            guarantor_phone: p.guarantor_phone,
            guarantor_relationship: p.guarantor_relationship,
            guarantor_photo: p.guarantor_photo,
            licence_number: p.licence_number,
            licence_class: p.licence_class,
            licence_expiry_date: p.licence_expiry_date,
            licence_photo: p.licence_photo,
            track: p.track,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_driver_dto() -> DriverRegistrationDto {
        let future_date = chrono::Utc::now().date_naive() + chrono::Duration::days(180);
        DriverRegistrationDto {
            nin: "12345678901".to_string(),
            nin_slip: "data:image/png;base64,sample_slip".to_string(),
            profile_photo: "data:image/png;base64,sample_profile".to_string(),
            is_koinonia_worker: true,
            koinonia_department: Some("Transport".to_string()),
            guarantor_name: None,
            guarantor_phone: None,
            guarantor_relationship: None,
            guarantor_photo: None,
            licence_number: "KD-1234567".to_string(),
            licence_class: LicenceClass::B,
            licence_expiry_date: future_date,
            licence_photo: "data:image/png;base64,sample_licence".to_string(),
            vehicle_make: "Toyota".to_string(),
            vehicle_model: "Corolla".to_string(),
            vehicle_colour: "Silver".to_string(),
            vehicle_year: 2020,
            vehicle_plate_number: "ABC-123-XY".to_string(),
            passenger_seats: PassengerSeats(4),
            vehicle_photo: Some("data:image/png;base64,sample_vehicle".to_string()),
            vehicle_registration: "data:image/png;base64,sample_reg".to_string(),
            insurance_certificate: "data:image/png;base64,sample_ins".to_string(),
            roadworthiness_certificate: "data:image/png;base64,sample_road".to_string(),
            track: DriverTrack::Both,
        }
    }

    #[test]
    fn test_valid_koinonia_worker() {
        let dto = valid_driver_dto();
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn test_valid_non_koinonia_worker() {
        let mut dto = valid_driver_dto();
        dto.is_koinonia_worker = false;
        dto.koinonia_department = None;
        dto.guarantor_name = Some("Jane Doe".to_string());
        dto.guarantor_phone = Some("+2348011112222".to_string());
        dto.guarantor_relationship = Some("Sister".to_string());
        dto.guarantor_photo = Some("data:image/png;base64,guarantor_pic".to_string());

        assert!(dto.validate().is_ok());
    }

    #[test]
    fn test_invalid_nin() {
        let mut dto = valid_driver_dto();
        dto.nin = "123456".to_string(); // too short
        assert!(dto.validate().is_err());

        dto.nin = "1234567890A".to_string(); // non-digit
        assert!(dto.validate().is_err());
    }

    #[test]
    fn test_missing_koinonia_department() {
        let mut dto = valid_driver_dto();
        dto.is_koinonia_worker = true;
        dto.koinonia_department = None;
        assert!(dto.validate().is_err());

        dto.koinonia_department = Some("   ".to_string());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn test_missing_guarantor_info() {
        let mut dto = valid_driver_dto();
        dto.is_koinonia_worker = false;
        dto.guarantor_name = None;
        assert!(dto.validate().is_err());

        dto.guarantor_name = Some("Jane Doe".to_string());
        dto.guarantor_phone = None;
        assert!(dto.validate().is_err());

        dto.guarantor_phone = Some("+2348011112222".to_string());
        dto.guarantor_relationship = None;
        assert!(dto.validate().is_err());

        dto.guarantor_relationship = Some("Brother".to_string());
        dto.guarantor_photo = None;
        assert!(dto.validate().is_err());
    }

    #[test]
    fn test_licence_expiry_less_than_3_months() {
        let mut dto = valid_driver_dto();
        // 30 days from now is less than 3 months
        dto.licence_expiry_date = chrono::Utc::now().date_naive() + chrono::Duration::days(30);
        assert!(dto.validate().is_err());
    }

    #[test]
    fn test_invalid_plate_number() {
        let mut dto = valid_driver_dto();
        dto.vehicle_plate_number = "ABC123XY".to_string();
        assert!(dto.validate().is_err());

        dto.vehicle_plate_number = "123-ABC-XY".to_string();
        assert!(dto.validate().is_err());

        dto.vehicle_plate_number = "ABC-123-X".to_string();
        assert!(dto.validate().is_err());

        dto.vehicle_plate_number = "abc-123-xy".to_string(); // case insensitive
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn test_passenger_seats() {
        let mut dto = valid_driver_dto();
        dto.passenger_seats = PassengerSeats(2);
        assert!(dto.validate().is_err());

        dto.passenger_seats = PassengerSeats(3);
        assert!(dto.validate().is_ok());

        dto.passenger_seats = PassengerSeats(6);
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn test_passenger_seats_deserialization() {
        #[derive(Deserialize)]
        struct S {
            seats: PassengerSeats,
        }

        let s1: S = serde_json::from_str(r#"{"seats": 4}"#).unwrap();
        assert_eq!(s1.seats.0, 4);

        let s2: S = serde_json::from_str(r#"{"seats": "5"}"#).unwrap();
        assert_eq!(s2.seats.0, 5);

        let s3: S = serde_json::from_str(r#"{"seats": "6+"}"#).unwrap();
        assert_eq!(s3.seats.0, 6);
    }

    #[test]
    fn test_vehicle_documents_required() {
        let mut dto = valid_driver_dto();
        dto.vehicle_registration = "".to_string();
        assert!(dto.validate().is_err());

        let mut dto = valid_driver_dto();
        dto.insurance_certificate = "".to_string();
        assert!(dto.validate().is_err());

        let mut dto = valid_driver_dto();
        dto.roadworthiness_certificate = "".to_string();
        assert!(dto.validate().is_err());
    }

    #[test]
    fn test_vehicle_year_bounds() {
        let mut dto = valid_driver_dto();
        dto.vehicle_year = 1989;
        assert!(dto.validate().is_err());

        let current_year = chrono::Utc::now().year();
        dto.vehicle_year = current_year + 2;
        assert!(dto.validate().is_err());

        dto.vehicle_year = current_year + 1;
        assert!(dto.validate().is_ok());

        dto.vehicle_year = 1990;
        assert!(dto.validate().is_ok());
    }
}
