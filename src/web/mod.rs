use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

pub mod handlers;

use crate::AppContext;

pub async fn start_server(ctx: Arc<AppContext>, addr: SocketAddr) -> anyhow::Result<()> {
    let app = handlers::router(ctx);

    info!("Starting server on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}