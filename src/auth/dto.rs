use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginPayload {
    pub user: String,
    pub password: String,
}
