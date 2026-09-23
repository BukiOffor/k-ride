use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LoginPayload {
    pub user: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LoginResponseDto {
    pub user: crate::users::UserDto,
    pub token: String,
    pub refresh_token: String,
    pub token_type: String,
}
