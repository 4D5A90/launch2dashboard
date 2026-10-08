use launch2dashboard::{
    icons::FsIconStore,
    launchd::MacLaunchd,
    web::{self, Shutdown},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Err("launch2dashboard requires macOS".into());
    }
    let manager = MacLaunchd::from_environment().map_err(|error| error.message)?;
    let icons = FsIconStore::from_environment().map_err(|error| error.message)?;
    eprintln!("Managing launchd domain {}", manager.launch_domain());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await?;
    eprintln!("launch2dashboard is listening at http://127.0.0.1:9090");
    let (stop, shutdown) = Shutdown::channel();
    axum::serve(
        listener,
        web::router(Arc::new(manager), Arc::new(icons), shutdown),
    )
    .with_graceful_shutdown(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = stop.send(true);
    })
    .await?;
    Ok(())
}
