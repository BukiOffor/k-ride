use super::*;
use diesel_async::RunQueryDsl;

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

    Ok(UserDto::from(user))
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
    let root_email = std::env::var("ROOT_EMAIL").unwrap_or_else(|_| "root@freebus.com".to_string());
    let root_username = std::env::var("ROOT_USERNAME").unwrap_or_else(|_| "root".to_string());
    let root_password = std::env::var("ROOT_PASSWORD").unwrap_or_else(|_| "iamroot".to_string());
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
        country: "Nigeria".to_string(),
        identity_type: Some("System".to_string()),
        identity_number: Some("ROOT-001".to_string()),
        password_hash: hashed_password,
        is_identity_verified: true,
        role: RoleType::Root,
        is_active: true,
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
