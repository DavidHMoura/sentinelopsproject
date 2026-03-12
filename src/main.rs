use actix_web::{web, App, HttpServer};

mod api;
mod config;
mod db;
mod detection;
mod errors;
mod ingestor;
mod middleware;
mod models;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = config::Config::from_env().expect("Failed to load configuration");
    let pool = db::create_pool(&config.database_url)
        .await
        .expect("Failed to create database pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    let detection_engine = web::Data::new(detection::DetectionEngine::new());
    let async_ingestor = web::Data::new(ingestor::AsyncIngestor::new(pool.clone()));
    let app_config = web::Data::new(config.clone());
    let db_pool = web::Data::new(pool);

    tracing::info!("Starting SentinelOps v0.3.0 Ultra on {}:{}", config.server_host, config.server_port);

    HttpServer::new(move || {
        App::new()
            .app_data(app_config.clone())
            .app_data(db_pool.clone())
            .app_data(detection_engine.clone())
            .app_data(async_ingestor.clone())
            .app_data(detection_engine.clone())
            .app_data(async_ingestor.clone())
            .wrap(middleware::ApiKeyAuth::new(app_config.api_keys.clone()))
            .service(
                web::scope("/api")
                    .service(api::ingest_event)
                    .service(api::list_events)
                    .service(api::list_alerts)
            )
    })
    .bind((config.server_host, config.server_port))?
    .run()
    .await
}
