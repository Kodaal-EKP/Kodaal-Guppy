use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

pub async fn run(start_watchers: bool) -> Result<(), Box<dyn std::error::Error>> {
    let app_state = crate::app::AppState::load()?;
    let addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        env::var("KODAAL_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(app_state.config.server.port),
    );
    if start_watchers {
        crate::watchers::spawn_watcher_loop(app_state.clone());
    }
    let router = crate::http_api::router(app_state);
    axum::Server::bind(&addr)
        .serve(router.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
