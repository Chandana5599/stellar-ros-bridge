//! Lightweight HTTP health and readiness endpoints.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use axum::{extract::State, http::StatusCode, routing::get, Router};

/// Shared state indicating whether required runtime dependencies are reachable.
#[derive(Clone)]
pub struct HealthState {
    dependency_ready: Arc<AtomicBool>,
}

impl HealthState {
    /// Creates health state with the initial dependency status.
    pub fn new(dependency_ready: bool) -> Self {
        Self {
            dependency_ready: Arc::new(AtomicBool::new(dependency_ready)),
        }
    }

    /// Updates dependency readiness when a dependency check reports a change.
    pub fn set_dependency_ready(&self, ready: bool) {
        self.dependency_ready.store(ready, Ordering::Relaxed);
    }

    fn is_dependency_ready(&self) -> bool {
        self.dependency_ready.load(Ordering::Relaxed)
    }
}

/// Builds the health endpoint router.
pub fn router(state: HealthState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state)
}

/// Reports whether the process is alive, without checking dependencies.
async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// Reports whether the runtime's required dependencies are ready.
async fn readyz(State(state): State<HealthState>) -> StatusCode {
    if state.is_dependency_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    use super::{router, HealthState};

    async fn get_status(app: Router, path: &str) -> StatusCode {
        let request = Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("valid test request");

        app.oneshot(request)
            .await
            .expect("router should handle the request")
            .status()
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let app = router(HealthState::new(false));

        assert_eq!(get_status(app, "/healthz").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_returns_ok_when_dependencies_are_ready() {
        let app = router(HealthState::new(true));

        assert_eq!(get_status(app, "/readyz").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_reflects_a_simulated_dependency_outage() {
        let state = HealthState::new(true);
        let app = router(state.clone());

        assert_eq!(get_status(app.clone(), "/readyz").await, StatusCode::OK);

        state.set_dependency_ready(false);

        assert_eq!(
            get_status(app.clone(), "/readyz").await,
            StatusCode::SERVICE_UNAVAILABLE
        );

        // Liveness must remain healthy even when a dependency is unavailable.
        assert_eq!(get_status(app.clone(), "/healthz").await, StatusCode::OK);

        state.set_dependency_ready(true);

        assert_eq!(get_status(app, "/readyz").await, StatusCode::OK);
    }
}
