use axum::http::{Method, header::*};
use diesel_migrations::{EmbeddedMigrations, embed_migrations};
use k_rides::AppState;
use k_rides::auth::swagger::AuthApiDoc;
use k_rides::bus::swagger::BusApiDoc;
use k_rides::logs::swagger::LogsApiDoc;
use k_rides::users::swagger::UsersApiDoc;
use std::sync::Arc;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_level(true)
        .with_env_filter("debug,tokio_postgres::query=off,tokio_postgres::prepare=off")
        .init();

    let pool: Arc<
        bb8::Pool<k_rides::AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>>,
    > = k_rides::config::create_pool().await.into();

    // Run database migrations
    if let Err(e) = run_migration().await {
        tracing::error!("{}", e.to_string());
        std::process::exit(1);
    }

    let (logger, logger_receiver) = async_channel::bounded::<k_rides::logs::ActivityLog>(10000);

    let state: Arc<AppState> = AppState {
        pool: pool.clone(),
        logger: logger.clone(),
    }
    .into();

    // Seed default root user if no root user exists
    if let Err(e) = k_rides::users::services::seed_root_user(state.clone()).await {
        tracing::error!("Failed to seed root user: {}", e);
    }

    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::PUT,
        ])
        .allow_headers([
            CONTENT_TYPE,
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
        ])
        .allow_credentials(true)
        .allow_origin([
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
            "http://localhost:5173".parse::<HeaderValue>().unwrap(),
            "https://ktvn-beryl.vercel.app".parse::<HeaderValue>().unwrap()

        ]);

    k_rides::config::start_log_workers(state.clone(), logger_receiver, 2).await;

    k_rides::info!("Starting Web Server ............");

    let app = k_rides::users::handlers::routes(state.clone())
        .merge(k_rides::bus::handlers::routes(state.clone()))
        .merge(k_rides::auth::handlers::routes(state.clone()))
        .merge(k_rides::logs::handlers::routes(state.clone()))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let mut openapi = UsersApiDoc::openapi();
    openapi.merge(BusApiDoc::openapi());
    openapi.merge(AuthApiDoc::openapi());
    openapi.merge(LogsApiDoc::openapi());

    let api = axum::Router::new()
        .merge(SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", openapi))
        .nest("/api/v1", app);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9990").await.unwrap();
    k_rides::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, api).await.unwrap();
}

pub async fn run_migration() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use diesel::prelude::*;
    use diesel_migrations::MigrationHarness;

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    tokio::task::spawn_blocking(move || {
        let mut conn = diesel::PgConnection::establish(&database_url)?;
        conn.run_pending_migrations(MIGRATIONS).map_err(|e| {
            tracing::error!("{}", e.to_string());
            std::process::exit(1);
        })?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await?
}
