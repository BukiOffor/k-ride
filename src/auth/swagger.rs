use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::login,
        super::password_reset::forgot,
        super::password_reset::reset,
        super::handlers::logout,
        super::handlers::refresh,
    ),
    components(schemas(
        super::dto::LoginPayload,
        super::password_reset::ForgotPasswordInput,
        super::password_reset::ResetPasswordInput,
        super::dto::LoginResponseDto,
        super::dto::RefreshTokenPayload,
    ))
)]
pub struct AuthApiDoc;
