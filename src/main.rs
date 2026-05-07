mod auth;
mod backup;
mod config;
mod error;
mod handlers;
mod state;
mod storage;

use crate::config::Config;
use crate::state::AppState;
use actix_files::Files;
use actix_session::config::PersistentSession;
use actix_session::storage::CookieSessionStore;
use actix_session::SessionMiddleware;
use actix_web::cookie::time::Duration as CookieDuration;
use actix_web::cookie::Key;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().map_err(io_err)?;
    tracing::info!(
        bind = %config.bind_addr,
        port = config.port,
        data = %config.data_dir.display(),
        backups = %config.backup_dir.display(),
        "starting mongodb-utils"
    );

    let state = Arc::new(AppState::init(&config).await.map_err(io_err)?);
    state.runner.ensure_dir().await.map_err(io_err)?;
    backup::scheduler::spawn(state.clone());

    let bind = (config.bind_addr.clone(), config.port);
    let session_key = Key::from(&config.session_key);
    let data: web::Data<AppState> = web::Data::from(state);

    HttpServer::new(move || {
        let session = SessionMiddleware::builder(CookieSessionStore::default(), session_key.clone())
            .cookie_name("mu_session".to_string())
            .cookie_secure(false) // set true if behind TLS
            .cookie_http_only(true)
            .cookie_same_site(actix_web::cookie::SameSite::Lax)
            .session_lifecycle(PersistentSession::default().session_ttl(CookieDuration::days(7)))
            .build();

        App::new()
            .app_data(data.clone())
            .app_data(web::JsonConfig::default().limit(1024 * 1024)) // 1 MiB JSON cap
            .wrap(Logger::default())
            .wrap(session)
            .service(
                web::scope("/api")
                    .service(
                        web::scope("/auth")
                            .route("/status", web::get().to(auth::handlers::status))
                            .route("/setup", web::post().to(auth::handlers::setup))
                            .route("/login", web::post().to(auth::handlers::login))
                            .route("/logout", web::post().to(auth::handlers::logout))
                            .route("/me", web::get().to(auth::handlers::me)),
                    )
                    .service(
                        web::scope("/connections")
                            .route("", web::get().to(handlers::connections::list))
                            .route("", web::post().to(handlers::connections::create))
                            .route("/test", web::post().to(handlers::connections::test))
                            .route("/{id}", web::put().to(handlers::connections::update))
                            .route("/{id}", web::delete().to(handlers::connections::delete))
                            .route(
                                "/{id}/databases",
                                web::get().to(handlers::connections::list_databases),
                            ),
                    )
                    .service(
                        web::scope("/schedules")
                            .route("", web::get().to(handlers::schedules::list))
                            .route("", web::post().to(handlers::schedules::upsert))
                            .route("/{id}", web::delete().to(handlers::schedules::delete)),
                    )
                    .service(
                        web::scope("/backups")
                            .route("", web::get().to(handlers::backups::list))
                            .route("/run", web::post().to(handlers::backups::run_now))
                            .route("/jobs", web::get().to(handlers::backups::jobs))
                            .route("/jobs/stream", web::get().to(handlers::backups::jobs_stream))
                            .route("/{name}", web::delete().to(handlers::backups::delete))
                            .route(
                                "/{name}/download",
                                web::get().to(handlers::backups::download),
                            ),
                    )
                    .service(
                        web::scope("/restore")
                            .app_data(actix_multipart::form::MultipartFormConfig::default()
                                .total_limit(2 * 1024 * 1024 * 1024)) // 2 GiB
                            .route("/server", web::post().to(handlers::restore::from_server))
                            .route("/upload", web::post().to(handlers::restore::from_upload)),
                    ),
            )
            .service(Files::new("/", "./static").index_file("index.html"))
    })
    .bind(bind)?
    .run()
    .await
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
