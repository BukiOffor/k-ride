use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::login,
        super::handlers::logout,
    ),
    components(schemas(
        super::dto::LoginPayload,
        super::dto::LoginResponseDto,
    ))
)]
pub struct AuthApiDoc;
