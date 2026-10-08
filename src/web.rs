use crate::domain::{
    AppError, ErrorKind, Icon, IconStore, MAX_ICON_BYTES, Service, ServiceAction, ServiceConfig,
    ServiceManager,
};
use askama::Template;
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        Html, IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use serde::Serialize;
use serde_json::json;
use std::sync::Mutex;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::watch;

/// Ends open event streams on shutdown, so a graceful stop does not wait on them forever.
#[derive(Clone)]
pub struct Shutdown(watch::Receiver<bool>);
impl Shutdown {
    pub fn channel() -> (watch::Sender<bool>, Self) {
        let (stop, requested) = watch::channel(false);
        (stop, Self(requested))
    }
    /// For servers that never stop gracefully, such as tests.
    pub fn never() -> Self {
        Self::channel().1
    }
    async fn requested(mut self) {
        // A dropped sender means nobody can ever request a shutdown.
        if self.0.wait_for(|stop| *stop).await.is_err() {
            std::future::pending::<()>().await
        }
    }
}

#[derive(Clone)]
struct AppState {
    manager: Arc<dyn ServiceManager>,
    icons: Arc<dyn IconStore>,
    shutdown: Shutdown,
    operations: Arc<Mutex<()>>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct Dashboard {
    services: Vec<Service>,
    selected_id: String,
    dev_reload: bool,
}

pub fn router(
    manager: Arc<dyn ServiceManager>,
    icons: Arc<dyn IconStore>,
    shutdown: Shutdown,
) -> Router {
    let routes = Router::new()
        .route("/", get(dashboard))
        .route("/services/{id}", get(service_page))
        .route("/static/style.css", get(stylesheet))
        .route("/static/app.js", get(javascript))
        .route("/api/services", get(list).post(create))
        .route("/api/services/{id}", get(detail).put(update).delete(delete))
        .route("/api/services/{id}/status", get(status))
        .route("/api/services/{id}/start", post(start))
        .route("/api/services/{id}/stop", post(stop))
        .route("/api/services/{id}/restart", post(restart))
        .route("/api/services/{id}/logs", get(logs))
        .route("/api/services/{id}/logs/stream", get(stream_logs))
        .route(
            "/api/services/{id}/icon",
            get(icon)
                .put(upload_icon)
                .delete(delete_icon)
                .layer(DefaultBodyLimit::max(MAX_ICON_BYTES)),
        );
    // Debug builds reload open pages when the dev server restarts (`bacon run`).
    #[cfg(debug_assertions)]
    let routes = routes.route("/dev/reload", get(dev_reload));
    routes
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))) })
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(local_requests_only))
        .with_state(AppState {
            manager,
            icons,
            shutdown,
            operations: Arc::new(Mutex::new(())),
        })
}

// Host validation also prevents DNS rebinding. A custom header on every mutation
// forces cross-origin browsers to preflight, for which we grant no CORS access.
async fn local_requests_only(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let allowed_host = matches!(host, Some("127.0.0.1:9090" | "localhost:9090"));
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let allowed_origin = origin
        .is_none_or(|value| matches!(value, "http://127.0.0.1:9090" | "http://localhost:9090"));
    let fetch_site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    let mutation = !matches!(*request.method(), Method::GET | Method::HEAD);
    let marked = headers
        .get("x-l2d-request")
        .is_some_and(|value| value == "1");
    if !allowed_host || !allowed_origin || fetch_site == Some("cross-site") || (mutation && !marked)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Only same-origin local requests are allowed"})),
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    let own_policy = response.extensions().get::<OwnPolicy>().is_some();
    let headers = response.headers_mut();
    if !own_policy {
        headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
}

/// Marks a response that sets its own CSP and cache policy instead of the global ones.
#[derive(Clone, Copy)]
struct OwnPolicy;

struct HttpError(AppError);

impl From<AppError> for HttpError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = match self.0.kind {
            ErrorKind::Validation => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Io | ErrorKind::Command => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({"error": self.0.message}))).into_response()
    }
}

async fn run<T, F>(state: &AppState, operation: F) -> Result<T, HttpError>
where
    T: Send + 'static,
    F: FnOnce(&dyn ServiceManager) -> Result<T, AppError> + Send + 'static,
{
    let manager = Arc::clone(&state.manager);
    let operations = Arc::clone(&state.operations);
    tokio::task::spawn_blocking(move || {
        // Hold the lock inside the worker so cancellation cannot release it
        // while a filesystem/launchctl mutation is still in progress.
        let _guard = operations.lock().map_err(|_| AppError {
            kind: ErrorKind::Io,
            message: "Service worker lock poisoned; restart launch2dashboard".into(),
        })?;
        operation(manager.as_ref())
    })
    .await
    .map_err(|error| {
        HttpError(AppError {
            kind: ErrorKind::Io,
            message: format!("Service worker failed: {error}"),
        })
    })?
    .map_err(HttpError)
}

async fn render(state: AppState, selected_id: String) -> Result<Html<String>, HttpError> {
    let check_id = selected_id.clone();
    let services = run(&state, move |manager| {
        if !check_id.is_empty() {
            manager.get(&check_id)?;
        }
        manager.list()
    })
    .await?;
    Dashboard {
        services,
        selected_id,
        dev_reload: cfg!(debug_assertions),
    }
    .render()
    .map(Html)
    .map_err(|error| {
        HttpError(AppError {
            kind: ErrorKind::Io,
            message: format!("Cannot render dashboard: {error}"),
        })
    })
}

async fn dashboard(State(state): State<AppState>) -> Result<Html<String>, HttpError> {
    render(state, String::new()).await
}
async fn service_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, HttpError> {
    render(state, id).await
}
/// A service as the dashboard shows it, with the version of its icon (if any).
#[derive(Serialize)]
struct ServiceView {
    #[serde(flatten)]
    service: Service,
    icon: Option<String>,
}
impl ServiceView {
    fn new(icons: &dyn IconStore, service: Service) -> Self {
        // An unreadable icon must not hide the service itself.
        let icon = icons.version(&service.config.id).ok().flatten();
        Self { service, icon }
    }
}
async fn list(State(state): State<AppState>) -> Result<Json<Vec<ServiceView>>, HttpError> {
    let icons = Arc::clone(&state.icons);
    run(&state, move |manager| {
        Ok(manager
            .list()?
            .into_iter()
            .map(|service| ServiceView::new(icons.as_ref(), service))
            .collect())
    })
    .await
    .map(Json)
}
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ServiceView>, HttpError> {
    let icons = Arc::clone(&state.icons);
    run(&state, move |manager| {
        Ok(ServiceView::new(icons.as_ref(), manager.get(&id)?))
    })
    .await
    .map(Json)
}
async fn status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HttpError> {
    run(&state, move |manager| {
        manager.get(&id).map(|service| service.status)
    })
    .await
    .map(Json)
}

async fn create(
    State(state): State<AppState>,
    Json(config): Json<ServiceConfig>,
) -> Result<impl IntoResponse, HttpError> {
    run(&state, move |manager| manager.create(config))
        .await
        .map(|service| (StatusCode::CREATED, Json(service)))
}
async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(config): Json<ServiceConfig>,
) -> Result<Json<Service>, HttpError> {
    run(&state, move |manager| manager.update(&id, config))
        .await
        .map(Json)
}
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let icons = Arc::clone(&state.icons);
    run(&state, move |manager| {
        manager.delete(&id)?;
        icons.delete(&id)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn icon(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, HttpError> {
    let icons = Arc::clone(&state.icons);
    let icon = run(&state, move |manager| {
        manager.get(&id)?;
        icons
            .get(&id)?
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "Service has no icon"))
    })
    .await?;
    Ok((
        Extension(OwnPolicy),
        [
            (header::CONTENT_TYPE, icon.format().media_type()),
            // An SVG opened directly must not run anything in the dashboard origin.
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; sandbox",
            ),
            // Clients request `?v=<version>`, which changes whenever the icon does.
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        icon.bytes().to_vec(),
    )
        .into_response())
}
async fn upload_icon(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, HttpError> {
    let icons = Arc::clone(&state.icons);
    run(&state, move |manager| {
        manager.get(&id)?;
        icons.put(&id, &Icon::parse(body.to_vec())?)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_icon(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let icons = Arc::clone(&state.icons);
    run(&state, move |manager| {
        manager.get(&id)?;
        icons.delete(&id)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn action(
    state: AppState,
    id: String,
    action: ServiceAction,
) -> Result<Json<Service>, HttpError> {
    run(&state, move |manager| manager.action(&id, action))
        .await
        .map(Json)
}
async fn start(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Service>, HttpError> {
    action(state, id, ServiceAction::Start).await
}
async fn stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Service>, HttpError> {
    action(state, id, ServiceAction::Stop).await
}
async fn restart(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Service>, HttpError> {
    action(state, id, ServiceAction::Restart).await
}
async fn logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HttpError> {
    run(&state, move |manager| manager.logs(&id))
        .await
        .map(Json)
}

async fn stream_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, HttpError> {
    let initial_id = id.clone();
    let initial = run(&state, move |manager| manager.logs(&initial_id)).await?;
    let stream = async_stream::stream! {
        let mut last = serde_json::to_string(&initial).unwrap_or_default();
        yield Ok::<Event, Infallible>(Event::default().data(last.clone()));
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        timer.tick().await;
        loop {
            tokio::select! {
                _ = timer.tick() => {}
                _ = state.shutdown.clone().requested() => break,
            }
            let log_id = id.clone();
            match run(&state, move |manager| manager.logs(&log_id)).await {
                Ok(snapshot) => {
                    let data = serde_json::to_string(&snapshot).unwrap_or_default();
                    if data != last {
                        yield Ok(Event::default().data(data.clone()));
                        last = data;
                    }
                }
                Err(error) => {
                    yield Ok(Event::default().event("service-error").data(error.0.message));
                    break;
                }
            }
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response())
}

/// Stays open until the server stops; the page reloads once it can reconnect.
#[cfg(debug_assertions)]
async fn dev_reload(State(state): State<AppState>) -> Response {
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().retry(Duration::from_millis(500)).comment("ready"));
        state.shutdown.requested().await;
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("static/style.css"),
    )
}
async fn javascript() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("static/app.js"),
    )
}
