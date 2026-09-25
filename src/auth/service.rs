use axum_extra::extract::CookieJar;
use cookie::Cookie;
use diesel_async::RunQueryDsl;

use super::jwt::*;
use super::*;

pub async fn login(
    jar: CookieJar,
    pool: Arc<Pool>,
    payload: dto::LoginPayload,
) -> Result<(CookieJar, dto::LoginResponseDto), ModuleError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| ModuleError::InternalError("could not get pool connection".into()))?;

    let user = crate::users::services::find_user_by_email_or_username(&mut conn, &payload.user)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(user) = user {
        let is_valid = crate::helpers::password_verfier(&payload.password, &user.password_hash);
        if !is_valid {
            return Err(ModuleError::AuthError);
        }

        let token = create_session_token(user.id, user.role.clone())?;

        let cookie = Cookie::build(("access_token", token.access_token.clone()))
            .http_only(true)
            .secure(true)
            .path("/")
            .max_age(cookie::time::Duration::days(7))
            .same_site(cookie::SameSite::None)
            .build();

        let refresh_cookie = Cookie::build(("refresh_token", token.refresh_token.clone()))
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(cookie::SameSite::None)
            .max_age(cookie::time::Duration::days(8))
            .build();

        let updated_jar = jar.add(cookie).add(refresh_cookie);

        diesel::update(schema::users::table.filter(schema::users::id.eq(user.id)))
            .set(schema::users::last_seen.eq(chrono::Utc::now().naive_utc()))
            .execute(&mut conn)
            .await?;

        let response = dto::LoginResponseDto {
            user: crate::users::UserDto::from(user),
            token: token.access_token,
            refresh_token: token.refresh_token,
            token_type: "Bearer".to_string(),
        };

        Ok((updated_jar, response))
    } else {
        Err(ModuleError::AuthError)
    }
}

pub fn logout(jar: CookieJar) -> CookieJar {
    let cookie = Cookie::build(("access_token", ""))
        .http_only(true)
        .secure(true)
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .same_site(cookie::SameSite::None)
        .build();

    let refresh_cookie = Cookie::build(("refresh_token", ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(cookie::SameSite::None)
        .max_age(cookie::time::Duration::seconds(0))
        .build();

    jar.add(cookie).add(refresh_cookie)
}

pub async fn refresh(
    jar: CookieJar,
    pool: Arc<Pool>,
    token_str: Option<String>,
) -> Result<(CookieJar, dto::LoginResponseDto), ModuleError> {
    let refresh_token = token_str
        .or_else(|| jar.get("refresh_token").map(|c| c.value().to_string()))
        .ok_or(ModuleError::CouldNotExtractToken(
            "Missing refresh token in cookie, header, or body.",
        ))?;

    let claims = decode_token(&refresh_token)?;

    let mut conn = pool
        .get()
        .await
        .map_err(|_| ModuleError::InternalError("could not get pool connection".into()))?;

    use crate::schema::users::dsl as users_dsl;
    let user = users_dsl::users
        .filter(users_dsl::id.eq(claims.user_id))
        .first::<crate::users::User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ModuleError::AuthError,
            _ => ModuleError::InternalError(e.to_string().into()),
        })?;

    if !user.is_active {
        return Err(ModuleError::NotAllowed("Account is inactive"));
    }

    let token = create_session_token(user.id, user.role.clone())?;

    let cookie = Cookie::build(("access_token", token.access_token.clone()))
        .http_only(true)
        .secure(true)
        .path("/")
        .max_age(cookie::time::Duration::days(7))
        .same_site(cookie::SameSite::None)
        .build();

    let refresh_cookie = Cookie::build(("refresh_token", token.refresh_token.clone()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(cookie::SameSite::None)
        .max_age(cookie::time::Duration::days(8))
        .build();

    let updated_jar = jar.add(cookie).add(refresh_cookie);

    diesel::update(users_dsl::users.filter(users_dsl::id.eq(user.id)))
        .set(users_dsl::last_seen.eq(chrono::Utc::now().naive_utc()))
        .execute(&mut conn)
        .await?;

    let response = dto::LoginResponseDto {
        user: crate::users::UserDto::from(user),
        token: token.access_token,
        refresh_token: token.refresh_token,
        token_type: "Bearer".to_string(),
    };

    Ok((updated_jar, response))
}
