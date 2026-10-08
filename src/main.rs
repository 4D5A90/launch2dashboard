use launch2dashboard::{launchd::MacLaunchd, web};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Err("launch2dashboard requires macOS and a logged-in GUI user session".into());
    }
    let manager = MacLaunchd::from_environment().map_err(|error| error.message)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await?;
    eprintln!("launch2dashboard is listening at http://127.0.0.1:9090");
    axum::serve(listener, web::router(Arc::new(manager)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
