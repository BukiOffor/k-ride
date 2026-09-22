#![allow(unused_variables)]

use crate::{auth::jwt::Claims, users::RoleType};
use axum::{
    debug_middleware,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
pub use tokio::task_local;

task_local! {
    pub static USER: String;
}

#[debug_middleware]
pub async fn authorize(
    State(state): State<Arc<crate::AppState>>,
    Claims { .. }: Claims,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    Ok(next.run(req).await)
}

pub async fn admin_authorize(
    Claims { role, .. }: Claims,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !matches!(role, RoleType::Admin | RoleType::Root) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(req).await)
}

pub async fn coordinator_authorize(
    Claims { role, .. }: Claims,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !matches!(role, RoleType::RouteCoordinator | RoleType::Root | RoleType::Admin) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(req).await)
}


pub async fn connection_info_middleware(
    user_agent: Option<axum_extra::TypedHeader<axum_extra::headers::UserAgent>>,
    addr: axum::extract::ConnectInfo<core::net::SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let user_agent = match user_agent {
        Some(u) => u,
        None => {
            return axum::response::IntoResponse::into_response(axum::response::Html(
                "UserAgent is missing.",
            ));
        }
    };
    request.extensions_mut().insert(ConnectionInfo {
        ip: addr.to_string(),
        user_agent: user_agent.to_string(),
    });
    next.run(request).await
}

#[derive(Clone)]
pub struct ConnectionInfo {
    pub ip: String,
    pub user_agent: String,
}
