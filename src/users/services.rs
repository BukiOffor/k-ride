use super::*;
use crate::vrp::models::cars::{Car, CarSize, CarTier, CarType};
use crate::vrp::models::drivers::DriverProfile;
use diesel_async::RunQueryDsl;

pub fn map_car_specs(seats: i32) -> (CarType, CarTier, CarSize) {
    let car_size = if seats <= 2 {
        CarSize::Standard
    } else if seats <= 4 {
        CarSize::Large
    } else {
        CarSize::XtraLarge
    };
    let car_type = if seats >= 6 {
        CarType::Van
    } else if seats >= 5 {
        CarType::SUV
    } else {
        CarType::Saloon
    };
    (car_type, CarTier::Comfort, car_size)
}

async fn upsert_car_from_driver_details(
    conn: &mut crate::AsyncPgConnection,
    driver_details: &DriverRegistrationDto,
    existing_car_id: Option<Uuid>,
) -> Result<Uuid, ApiError> {
    use crate::schema::cars::dsl::*;
    let now = chrono::Utc::now().naive_utc();
    let plate = driver_details.vehicle_plate_number.trim().to_uppercase();
    let seats: i32 = driver_details.passenger_seats.into();
    let (c_type, c_tier, c_size) = map_car_specs(seats);

    let target_car = if let Some(target_id) = existing_car_id {
        cars.filter(id.eq(target_id))
            .first::<Car>(conn)
            .await
            .optional()
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?
    } else {
        cars.filter(license_plate.eq(&plate))
            .first::<Car>(conn)
            .await
            .optional()
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?
    };

    if let Some(existing) = target_car {
        let updated = diesel::update(cars.filter(id.eq(existing.id)))
            .set((
                license_plate.eq(&plate),
                capacity.eq(seats),
                car_type.eq(c_type),
                car_tier.eq(c_tier),
                car_size.eq(c_size),
                color.eq(driver_details.vehicle_colour.trim()),
                brand.eq(driver_details.vehicle_make.trim()),
                model.eq(driver_details.vehicle_model.trim()),
                year.eq(driver_details.vehicle_year),
                vehicle_photo.eq(&driver_details.vehicle_photo),
                vehicle_registration.eq(Some(&driver_details.vehicle_registration)),
                insurance_certificate.eq(Some(&driver_details.insurance_certificate)),
                roadworthiness_certificate.eq(Some(&driver_details.roadworthiness_certificate)),
                updated_at.eq(now),
            ))
            .get_result::<Car>(conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        Ok(updated.id)
    } else {
        let new_id = Uuid::now_v7();
        let new_car = Car {
            id: new_id,
            license_plate: plate,
            capacity: seats,
            car_type: c_type,
            car_tier: c_tier,
            car_size: c_size,
            color: driver_details.vehicle_colour.trim().to_string(),
            color_hex: None,
            brand: driver_details.vehicle_make.trim().to_string(),
            model: driver_details.vehicle_model.trim().to_string(),
            year: driver_details.vehicle_year,
            vehicle_photo: driver_details.vehicle_photo.clone(),
            vehicle_registration: Some(driver_details.vehicle_registration.clone()),
            insurance_certificate: Some(driver_details.insurance_certificate.clone()),
            roadworthiness_certificate: Some(driver_details.roadworthiness_certificate.clone()),
            is_verified: false,
            created_at: now,
            updated_at: now,
        };
        diesel::insert_into(cars)
            .values(&new_car)
            .execute(conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        Ok(new_id)
    }
}

pub async fn create_user(
    state: Arc<AppState>,
    payload: AdminCreateUserDto,
) -> Result<Message<UserDto>, ApiError> {
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate email doesn't exist
    let user_exists = crate::schema::users::dsl::users
        .filter(crate::schema::users::dsl::email.eq(&payload.email))
        .first::<User>(&mut conn)
        .await
        .is_ok();

    if user_exists {
        return Err(ApiError::Error(
            "User with this email already exists".into(),
        ));
    }

    // Validate username doesn't exist
    let username_exists = crate::schema::users::dsl::users
        .filter(crate::schema::users::dsl::username.eq(&payload.username))
        .first::<User>(&mut conn)
        .await
        .is_ok();

    if username_exists {
        return Err(ApiError::Error(
            "User with this username already exists".into(),
        ));
    }

    if payload.role == RoleType::Driver && payload.driver_details.is_some() {
        let driver_details = payload.driver_details.as_ref().unwrap();
        driver_details.validate()?;

        let now = chrono::Utc::now().naive_utc();
        let car_id = upsert_car_from_driver_details(&mut conn, driver_details, payload.car).await?;

        let password_hash = crate::helpers::password_hasher(&payload.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        let user_id = Uuid::now_v7();

        let new_user = User {
            id: user_id,
            first_name: payload.first_name,
            last_name: payload.last_name,
            email: payload.email,
            username: payload.username,
            phone: payload.phone,
            address: payload.address,
            country: payload.country,
            city: payload.city.unwrap_or_else(|| "N/A".to_string()),
            identity_type: Some("NIN".to_string()),
            identity_number: Some(driver_details.nin.trim().to_string()),
            password_hash,
            is_identity_verified: false,
            role: RoleType::Driver,
            is_active: payload.is_active.unwrap_or(true),
            car: Some(car_id),
            account_number: payload.account_number,
            bank_name: payload.bank_name,
            created_at: now,
            updated_at: now,
            last_seen: now,
        };

        let user = diesel::insert_into(crate::schema::users::dsl::users)
            .values(&new_user)
            .get_result::<User>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        let new_profile = DriverProfile {
            id: Uuid::now_v7(),
            user_id,
            car_id,
            nin_slip: driver_details.nin_slip.clone(),
            profile_photo: driver_details.profile_photo.clone(),
            is_koinonia_worker: driver_details.is_koinonia_worker,
            koinonia_department: driver_details.koinonia_department.clone(),
            guarantor_name: driver_details.guarantor_name.clone(),
            guarantor_phone: driver_details.guarantor_phone.clone(),
            guarantor_relationship: driver_details.guarantor_relationship.clone(),
            guarantor_photo: driver_details.guarantor_photo.clone(),
            licence_number: driver_details.licence_number.trim().to_string(),
            licence_class: driver_details.licence_class.to_string(),
            licence_expiry_date: driver_details.licence_expiry_date,
            licence_photo: driver_details.licence_photo.clone(),
            track: driver_details.track.to_string(),
            status: "Pending".to_string(),
            created_at: now,
            updated_at: now,
        };

        let profile = diesel::insert_into(crate::schema::driver_profiles::dsl::driver_profiles)
            .values(&new_profile)
            .get_result::<DriverProfile>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        let user_dto = UserDto::from(user).with_driver_profile(profile.into());

        Ok(Message::context("User created successfully.", user_dto))
    } else {
        // Create User object from DTO
        let new_user: User = payload.try_into()?;

        // Insert user into DB
        let user = diesel::insert_into(crate::schema::users::dsl::users)
            .values(&new_user)
            .get_result::<User>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        Ok(Message::context(
            "User created successfully.",
            UserDto::from(user),
        ))
    }
}

pub async fn find_user_by_email_or_username(
    conn: &mut crate::AsyncPgConnection,
    identifier: &str,
) -> Result<Option<User>, ApiError> {
    use crate::schema::users::dsl::*;

    let user = users
        .filter(email.eq(identifier).or(username.eq(identifier)))
        .first::<User>(conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(user)
}

pub async fn assign_role_to_user(
    state: Arc<AppState>,
    user_id: Uuid,
    new_role: RoleType,
) -> Result<UserDto, ApiError> {
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Verify user exists
    let _ = crate::schema::users::dsl::users
        .filter(crate::schema::users::dsl::id.eq(user_id))
        .first::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let updated_user = diesel::update(
        crate::schema::users::dsl::users.filter(crate::schema::users::dsl::id.eq(user_id)),
    )
    .set((
        crate::schema::users::dsl::role.eq(new_role),
        crate::schema::users::dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
    ))
    .get_result::<User>(&mut conn)
    .await
    .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(UserDto::from(updated_user))
}

pub async fn register_user(
    state: Arc<AppState>,
    payload: CreateUserDto,
) -> Result<Message<UserDto>, ApiError> {
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate email doesn't exist
    let user_exists = crate::schema::users::dsl::users
        .filter(crate::schema::users::dsl::email.eq(&payload.email))
        .first::<User>(&mut conn)
        .await
        .is_ok();

    if user_exists {
        return Err(ApiError::Error(
            "User with this email already exists".into(),
        ));
    }

    // Validate username doesn't exist
    let username_exists = crate::schema::users::dsl::users
        .filter(crate::schema::users::dsl::username.eq(&payload.username))
        .first::<User>(&mut conn)
        .await
        .is_ok();

    if username_exists {
        return Err(ApiError::Error(
            "User with this username already exists".into(),
        ));
    }

    let is_driver = payload.role == Some(RoleType::Driver) || payload.driver_details.is_some();
    if is_driver {
        let driver_details = payload.driver_details.as_ref().ok_or_else(|| {
            ApiError::BadRequest("Driver registration details are required for driver accounts")
        })?;
        driver_details.validate()?;

        let now = chrono::Utc::now().naive_utc();
        let car_id = upsert_car_from_driver_details(&mut conn, driver_details, None).await?;

        let password_hash = crate::helpers::password_hasher(&payload.password)
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        let user_id = Uuid::now_v7();

        let new_user = User {
            id: user_id,
            first_name: payload.first_name,
            last_name: payload.last_name,
            email: payload.email,
            username: payload.username,
            phone: payload.phone,
            address: payload.address,
            country: payload.country,
            city: payload.city.unwrap_or_else(|| "N/A".to_string()),
            identity_type: Some("NIN".to_string()),
            identity_number: Some(driver_details.nin.trim().to_string()),
            password_hash,
            is_identity_verified: false,
            is_active: true,
            role: RoleType::Driver,
            car: Some(car_id),
            account_number: payload.account_number,
            bank_name: payload.bank_name,
            created_at: now,
            updated_at: now,
            last_seen: now,
        };

        let user = diesel::insert_into(crate::schema::users::dsl::users)
            .values(&new_user)
            .get_result::<User>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        let new_profile = DriverProfile {
            id: Uuid::now_v7(),
            user_id,
            car_id,
            nin_slip: driver_details.nin_slip.clone(),
            profile_photo: driver_details.profile_photo.clone(),
            is_koinonia_worker: driver_details.is_koinonia_worker,
            koinonia_department: driver_details.koinonia_department.clone(),
            guarantor_name: driver_details.guarantor_name.clone(),
            guarantor_phone: driver_details.guarantor_phone.clone(),
            guarantor_relationship: driver_details.guarantor_relationship.clone(),
            guarantor_photo: driver_details.guarantor_photo.clone(),
            licence_number: driver_details.licence_number.trim().to_string(),
            licence_class: driver_details.licence_class.to_string(),
            licence_expiry_date: driver_details.licence_expiry_date,
            licence_photo: driver_details.licence_photo.clone(),
            track: driver_details.track.to_string(),
            status: "Pending".to_string(),
            created_at: now,
            updated_at: now,
        };

        let profile = diesel::insert_into(crate::schema::driver_profiles::dsl::driver_profiles)
            .values(&new_profile)
            .get_result::<DriverProfile>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        let user_dto = UserDto::from(user).with_driver_profile(profile.into());

        Ok(Message::context("User created successfully.", user_dto))
    } else {
        // Create User object from DTO
        let new_user: User = payload.try_into()?;

        // Insert user into DB
        let user = diesel::insert_into(crate::schema::users::dsl::users)
            .values(&new_user)
            .get_result::<User>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

        Ok(Message::context(
            "User created successfully.",
            UserDto::from(user),
        ))
    }
}

pub async fn get_users(
    state: Arc<AppState>,
    params: GetUsersQuery,
) -> Result<Vec<UserDto>, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::users::table.into_boxed::<diesel::pg::Pg>();

    // Search by username, email and names
    if let Some(ref s) = params.search {
        let s = s.trim();
        if !s.is_empty() {
            let words: Vec<&str> = s.split_whitespace().collect();
            for word in words {
                let pattern = format!("%{}%", word);
                query = query.filter(
                    email
                        .ilike(pattern.clone())
                        .or(username.ilike(pattern.clone()))
                        .or(first_name.ilike(pattern.clone()))
                        .or(last_name.ilike(pattern)),
                );
            }
        }
    }

    // Filter by role
    if let Some(ref r) = params.role {
        query = query.filter(role.eq(r));
    }

    // Filter by is_active
    if let Some(active) = params.is_active {
        query = query.filter(is_active.eq(active));
    }

    // Order by newest first
    query = query.order(created_at.desc());

    // Pagination (if pagination is empty, gets all users)
    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<User>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result.into_iter().map(UserDto::from).collect())
}

pub async fn get_user_by_id(state: Arc<AppState>, user_id: Uuid) -> Result<UserDto, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let user = users
        .filter(id.eq(user_id))
        .first::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut user_dto = UserDto::from(user);
    if user_dto.role == RoleType::Driver {
        let profile = crate::schema::driver_profiles::dsl::driver_profiles
            .filter(crate::schema::driver_profiles::dsl::user_id.eq(user_id))
            .first::<DriverProfile>(&mut conn)
            .await
            .optional()
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        if let Some(p) = profile {
            user_dto = user_dto.with_driver_profile(p.into());
        }
    }

    Ok(user_dto)
}

pub async fn update_user(
    state: Arc<AppState>,
    user_id: Uuid,
    payload: UpdateUserDto,
) -> Result<UserDto, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_user = diesel::update(users.filter(id.eq(user_id)))
        .set((&payload, updated_at.eq(chrono::Utc::now().naive_utc())))
        .get_result::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(UserDto::from(updated_user))
}

pub async fn activate_user(state: Arc<AppState>, user_id: Uuid) -> Result<UserDto, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_user = diesel::update(users.filter(id.eq(user_id)))
        .set((
            is_active.eq(true),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(UserDto::from(updated_user))
}

pub async fn deactivate_user(state: Arc<AppState>, user_id: Uuid) -> Result<UserDto, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_user = diesel::update(users.filter(id.eq(user_id)))
        .set((
            is_active.eq(false),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(UserDto::from(updated_user))
}

pub async fn delete_user(state: Arc<AppState>, user_id: Uuid) -> Result<UserDto, ApiError> {
    use crate::schema::users::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted_user = diesel::delete(users.filter(id.eq(user_id)))
        .get_result::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(UserDto::from(deleted_user))
}

pub async fn seed_root_user(state: Arc<AppState>) -> Result<(), ApiError> {
    use crate::schema::users::dsl::*;
    use diesel::OptionalExtension;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Check if any root user already exists
    let root_user_exists = users
        .filter(role.eq(RoleType::Root))
        .first::<User>(&mut conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    if root_user_exists.is_some() {
        tracing::info!("Root user already exists. Skipping root user seeding.");
        return Ok(());
    }

    // Read root user credentials from environment or fall back to defaults
    let root_email =
        std::env::var("ROOT_EMAIL").unwrap_or_else(|_| "root@freebus.cloud".to_string());
    let root_username = std::env::var("ROOT_USERNAME").unwrap_or_else(|_| "root".to_string());
    let root_password = std::env::var("ROOT_PASSWORD").unwrap_or_else(|_| "@iamroot".to_string());
    let root_phone = std::env::var("ROOT_PHONE").unwrap_or_else(|_| "+2348000000000".to_string());
    let root_first_name = std::env::var("ROOT_FIRST_NAME").unwrap_or_else(|_| "System".to_string());
    let root_last_name = std::env::var("ROOT_LAST_NAME").unwrap_or_else(|_| "Root".to_string());

    let hashed_password = crate::helpers::password_hasher(&root_password)
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let now = chrono::Utc::now().naive_utc();
    let new_root = User {
        id: Uuid::now_v7(),
        first_name: root_first_name,
        last_name: root_last_name,
        email: root_email.clone(),
        username: root_username.clone(),
        phone: root_phone,
        address: "System Headquarters".to_string(),
        city: "Abuja".to_string(),
        country: "Nigeria".to_string(),
        identity_type: None,
        identity_number: None,
        password_hash: hashed_password,
        is_identity_verified: true,
        role: RoleType::Root,
        is_active: true,
        car: None,
        account_number: None,
        bank_name: None,
        created_at: now,
        updated_at: now,
        last_seen: now,
    };

    diesel::insert_into(users)
        .values(&new_root)
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    tracing::info!(
        "Seeded default Root user: username='{}', email='{}'",
        root_username,
        root_email
    );

    Ok(())
}
