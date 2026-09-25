use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::login,
        super::handlers::logout,
        super::handlers::refresh,
    ),
    components(schemas(
        super::dto::LoginPayload,
        super::dto::LoginResponseDto,
        super::dto::RefreshTokenPayload,
    ))
)]
pub struct AuthApiDoc;
