use super::*;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

pub fn password_verfier(password: &str, hash: &str) -> bool {
    let argon2 = Argon2::default();
    let hash = PasswordHash::new(hash).unwrap();
    argon2.verify_password(password.as_bytes(), &hash).is_ok()
}

pub fn password_hasher(password: &str) -> Result<String, ModuleError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| ModuleError::InternalError("Could not has password for some reason".into()))?;
    Ok(password_hash.to_string())
}

pub fn parse_time_stamp(date_str: &str, time: &str) -> Result<NaiveDateTime, ModuleError> {
    let naive_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| ModuleError::Error("Invalid date format".into()))?;
    if !time.to_lowercase().contains("am") && !time.to_lowercase().contains("pm") {
        return Err(ModuleError::Error("Invalid time format".into()));
    }
    let meridian = time.contains("PM");
    let seq: Vec<_> = time.split(":").collect();
    let mut hour = seq[0]
        .parse::<u32>()
        .map_err(|_| ModuleError::Error("could not parse hour".into()))?;
    let minute = seq[1]
        .parse::<u32>()
        .map_err(|_| ModuleError::Error("could not parse minute".into()))?;
    if meridian {
        hour = seq[0]
            .parse::<u32>()
            .map_err(|_| ModuleError::Error("could not parse hours".into()))?
            + 12;
    }
    tracing::info!(r"{hour} : {minute}");
    let time = chrono::NaiveTime::from_hms_micro_opt(hour, minute, 0, 0)
        .ok_or("Invalid time format")
        .map_err(|e| ModuleError::Error(e.into()))?;
    let naive_datetime = naive_date.and_time(time);
    Ok(naive_datetime)
}
