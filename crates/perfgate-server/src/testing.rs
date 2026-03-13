//! Testing utilities for perfgate-server.
//!
//! This module is only available when compiled for tests.

use tokio::net::TcpListener;

use crate::auth::AuthState;
use crate::server::{ServerConfig, create_key_store, create_router, create_storage};

/// A running test server.
pub struct TestServer {
    /// The base URL of the server (e.g., "http://127.0.0.1:1234/api/v1")
    pub url: String,
    /// The root URL of the server (e.g., "http://127.0.0.1:1234")
    pub root_url: String,
    /// Join handle for the server task
    pub handle: tokio::task::JoinHandle<()>,
}

/// Spawns a real perfgate server on a random port for testing.
pub async fn spawn_test_server(config: ServerConfig) -> TestServer {
    let store = create_storage(&config)
        .await
        .expect("failed to create storage");
    let key_store = create_key_store(&config)
        .await
        .expect("failed to create key store");
    let auth_state = AuthState::new(key_store, config.jwt.clone());
    let app = create_router(store, auth_state, &config);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let root_url = format!("http://{}", addr);
    let url = format!("{}/api/v1", root_url);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    TestServer {
        url,
        root_url,
        handle,
    }
}
