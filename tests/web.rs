use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use http_body_util::BodyExt;
use launch2dashboard::{
    domain::*,
    web::{Shutdown, router},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

struct FakeManager {
    service: Service,
    calls: Mutex<Vec<String>>,
    log_reads: AtomicUsize,
    fail_later_logs: bool,
}

fn config(id: &str) -> ServiceConfig {
    ServiceConfig {
        id: id.into(),
        executable: "/bin/echo".into(),
        arguments: vec!["hello".into()],
        working_directory: None,
        environment: BTreeMap::new(),
        autostart: false,
        restart_on_failure: false,
    }
}
fn service(config: ServiceConfig) -> Service {
    Service {
        config,
        status: ServiceStatus {
            state: ServiceState::Running,
            pid: Some(42),
            uptime_seconds: Some(60),
            restart_count: None,
            last_exit_code: None,
            error: None,
        },
    }
}
impl FakeManager {
    fn new() -> Self {
        Self {
            service: service(config("example")),
            calls: Mutex::new(vec![]),
            log_reads: AtomicUsize::new(0),
            fail_later_logs: false,
        }
    }
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn check(&self, id: &str) -> Result<(), AppError> {
        if id == "missing" {
            Err(AppError::new(ErrorKind::NotFound, "Service missing"))
        } else {
            Ok(())
        }
    }
}
impl ServiceManager for FakeManager {
    fn list(&self) -> Result<Vec<Service>, AppError> {
        self.record("list".into());
        Ok(vec![self.service.clone()])
    }
    fn get(&self, id: &str) -> Result<Service, AppError> {
        self.record(format!("get:{id}"));
        self.check(id)?;
        Ok(self.service.clone())
    }
    fn create(&self, config: ServiceConfig) -> Result<Service, AppError> {
        self.record(format!("create:{}", config.id));
        config.validate()?;
        if config.id == "duplicate" {
            return Err(AppError::new(ErrorKind::Conflict, "Already exists"));
        }
        Ok(service(config))
    }
    fn update(&self, id: &str, config: ServiceConfig) -> Result<Service, AppError> {
        self.record(format!("update:{id}:{}", config.executable));
        self.check(id)?;
        config.validate()?;
        Ok(service(config))
    }
    fn delete(&self, id: &str) -> Result<(), AppError> {
        self.record(format!("delete:{id}"));
        self.check(id)
    }
    fn action(&self, id: &str, action: ServiceAction) -> Result<Service, AppError> {
        self.record(format!("action:{id}:{action:?}"));
        self.check(id)?;
        Ok(self.service.clone())
    }
    fn logs(&self, id: &str) -> Result<LogSnapshot, AppError> {
        self.check(id)?;
        if self.log_reads.fetch_add(1, Ordering::SeqCst) > 0 && self.fail_later_logs {
            return Err(AppError::new(ErrorKind::Io, "Log file disappeared"));
        }
        Ok(LogSnapshot {
            stdout: "ready\nnext line".into(),
            stderr: "warning".into(),
        })
    }
}
#[derive(Default)]
struct MemoryIcons {
    icons: Mutex<BTreeMap<String, (Icon, usize)>>,
    writes: AtomicUsize,
}
fn icons() -> Arc<MemoryIcons> {
    Arc::new(MemoryIcons::default())
}
impl IconStore for MemoryIcons {
    fn get(&self, id: &str) -> Result<Option<Icon>, AppError> {
        Ok(self
            .icons
            .lock()
            .unwrap()
            .get(id)
            .map(|(icon, _)| icon.clone()))
    }
    fn version(&self, id: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .icons
            .lock()
            .unwrap()
            .get(id)
            .map(|(_, v)| v.to_string()))
    }
    fn put(&self, id: &str, icon: &Icon) -> Result<(), AppError> {
        let version = self.writes.fetch_add(1, Ordering::SeqCst);
        self.icons
            .lock()
            .unwrap()
            .insert(id.into(), (icon.clone(), version));
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), AppError> {
        self.icons.lock().unwrap().remove(id);
        Ok(())
    }
}
const PNG: &[u8] = b"\x89PNG\r\n\x1a\npixels";
const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
fn upload(path: &str, bytes: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("host", "127.0.0.1:9090")
        .header("x-l2d-request", "1")
        .header("content-type", "application/octet-stream")
        .body(Body::from(bytes))
        .unwrap()
}
fn request(method: &str, path: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:9090");
    if method != "GET" {
        builder = builder.header("x-l2d-request", "1");
    }
    match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}
async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap()).unwrap()
}

#[tokio::test]
async fn rejects_untrusted_requests_before_touching_service_manager() {
    let manager = Arc::new(FakeManager::new());
    let app = router(manager.clone(), icons(), Shutdown::never());
    for (header, value) in [
        ("host", "attacker.test:9090"),
        ("host", "127.0.0.1:9090.attacker.test"),
        ("origin", "https://attacker.test"),
        ("origin", "null"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let mut req = request("GET", "/api/services", None);
        req.headers_mut().insert(
            axum::http::HeaderName::from_bytes(header.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "{header}={value}"
        );
    }
    let mut missing_host = request("GET", "/api/services", None);
    missing_host.headers_mut().remove("host");
    assert_eq!(
        app.clone().oneshot(missing_host).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    for marker in [None, Some("0")] {
        let mut req = request("POST", "/api/services/example/start", None);
        req.headers_mut().remove("x-l2d-request");
        if let Some(value) = marker {
            req.headers_mut()
                .insert("x-l2d-request", value.parse().unwrap());
        }
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert!(manager.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn same_origin_browser_and_local_cli_can_read_services_and_status() {
    let app = router(Arc::new(FakeManager::new()), icons(), Shutdown::never());
    let mut req = request("GET", "/api/services", None);
    req.headers_mut()
        .insert("origin", "http://127.0.0.1:9090".parse().unwrap());
    req.headers_mut()
        .insert("sec-fetch-site", "same-origin".parse().unwrap());
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await[0]["config"]["id"], "example");
    let response = app
        .clone()
        .oneshot(request("GET", "/api/services/example", None))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["status"]["pid"], 42);
    let response = app
        .oneshot(request("GET", "/api/services/example/status", None))
        .await
        .unwrap();
    let body = json_body(response).await;
    assert_eq!(body["state"], "running");
    assert_eq!(body["uptime_seconds"], 60);
}

#[tokio::test]
async fn creation_returns_created_and_domain_errors_keep_their_meaning() {
    let app = router(Arc::new(FakeManager::new()), icons(), Shutdown::never());
    for (id, expected) in [
        ("new-service", StatusCode::CREATED),
        ("../escape", StatusCode::BAD_REQUEST),
        ("duplicate", StatusCode::CONFLICT),
    ] {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/services",
                Some(serde_json::to_value(config(id)).unwrap()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{id}");
        let body = json_body(response).await;
        if expected == StatusCode::CREATED {
            assert_eq!(body["config"]["id"], id);
        } else {
            assert!(
                body["error"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            );
        }
    }
    let response = app
        .oneshot(request("GET", "/api/services/missing", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(response).await,
        json!({"error": "Service missing"})
    );
}

#[tokio::test]
async fn mutation_routes_dispatch_exact_target_and_action() {
    let manager = Arc::new(FakeManager::new());
    let app = router(manager.clone(), icons(), Shutdown::never());
    let mut updated = config("example");
    updated.executable = "/bin/sleep".into();
    let response = app
        .clone()
        .oneshot(request(
            "PUT",
            "/api/services/example",
            Some(serde_json::to_value(updated).unwrap()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["config"]["executable"],
        "/bin/sleep"
    );
    for action in ["start", "stop", "restart"] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/services/example/{action}"),
                    None
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    let response = app
        .oneshot(request("DELETE", "/api/services/example", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        to_bytes(response.into_body(), 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        *manager.calls.lock().unwrap(),
        [
            "update:example:/bin/sleep",
            "action:example:Start",
            "action:example:Stop",
            "action:example:Restart",
            "delete:example"
        ]
    );
}

#[tokio::test]
async fn static_assets_have_correct_types_and_security_headers() {
    let app = router(Arc::new(FakeManager::new()), icons(), Shutdown::never());
    for (path, content_type) in [
        ("/static/style.css", "text/css"),
        ("/static/app.js", "text/javascript"),
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", path, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with(content_type)
        );
        let policy = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(policy.contains("script-src 'self'"));
        assert!(!policy.contains("unsafe-inline"));
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    }
}

#[tokio::test]
async fn logs_stream_sends_initial_snapshot_and_reports_later_failure() {
    let mut manager = FakeManager::new();
    manager.fail_later_logs = true;
    let app = router(Arc::new(manager), icons(), Shutdown::never());
    let response = app
        .oneshot(request("GET", "/api/services/example/logs/stream", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut body = response.into_body();
    let first = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let frame = std::str::from_utf8(&first).unwrap();
    let data = frame
        .lines()
        .find_map(|line| {
            line.strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(data).unwrap(),
        json!({"stdout":"ready\nnext line", "stderr":"warning"})
    );
    let next = tokio::time::timeout(Duration::from_secs(3), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let frame = std::str::from_utf8(&next).unwrap();
    assert!(frame.contains("service-error"));
    assert!(frame.contains("Log file disappeared"));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), body.frame())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn missing_service_cannot_open_a_successful_logs_stream() {
    let response = router(Arc::new(FakeManager::new()), icons(), Shutdown::never())
        .oneshot(request("GET", "/api/services/missing/logs/stream", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rendered_service_data_cannot_inject_html_or_attributes() {
    let mut manager = FakeManager::new();
    manager.service.config.id = "\"><script>alert('id')</script>".into();
    manager.service.config.executable = "/bin/<img src=x onerror=alert('exec')>".into();
    let response = router(Arc::new(manager), icons(), Shutdown::never())
        .oneshot(request("GET", "/", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(
        to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(!html.contains("<script>alert('id')</script>"));
    assert!(!html.contains("<img src=x onerror="));
    assert!(
        html.contains("alert("),
        "The hostile fixture must actually be rendered to test escaping"
    );
}

#[tokio::test]
async fn uploaded_icon_is_listed_and_served_with_a_sandboxed_policy() {
    let store = icons();
    let app = router(
        Arc::new(FakeManager::new()),
        store.clone(),
        Shutdown::never(),
    );
    let send = |req| app.clone().oneshot(req);
    let listed_icon = |body: Value| body[0]["icon"].clone();
    assert_eq!(
        listed_icon(json_body(send(request("GET", "/api/services", None)).await.unwrap()).await),
        Value::Null
    );
    let response = send(upload("/api/services/example/icon", PNG.to_vec()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let first =
        listed_icon(json_body(send(request("GET", "/api/services", None)).await.unwrap()).await);
    assert!(first.is_string());
    let detail = json_body(
        send(request("GET", "/api/services/example", None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(detail["icon"], first);
    assert_eq!(detail["config"]["id"], "example");

    let response = send(request("GET", "/api/services/example/icon", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let policy = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        policy.contains("default-src 'none'") && policy.contains("sandbox"),
        "{policy}"
    );
    assert!(!policy.contains("script-src"), "{policy}");
    assert!(
        response.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("immutable")
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        PNG
    );

    send(upload("/api/services/example/icon", SVG.to_vec()))
        .await
        .unwrap();
    let response = send(request("GET", "/api/services/example/icon", None))
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "image/svg+xml");
    let second =
        listed_icon(json_body(send(request("GET", "/api/services", None)).await.unwrap()).await);
    assert_ne!(second, first);

    let response = send(request("DELETE", "/api/services/example/icon", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = send(request("GET", "/api/services/example/icon", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // The general policy still applies everywhere else.
    let response = send(request("GET", "/api/services", None)).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn icon_upload_rejects_bad_content_oversize_and_unknown_services() {
    let store = icons();
    let app = router(
        Arc::new(FakeManager::new()),
        store.clone(),
        Shutdown::never(),
    );
    let send = |req| app.clone().oneshot(req);
    send(upload("/api/services/example/icon", PNG.to_vec()))
        .await
        .unwrap();
    let response = send(upload("/api/services/example/icon", b"GIF89a".to_vec()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(store.get("example").unwrap().unwrap().bytes(), PNG);

    let mut oversize = PNG.to_vec();
    oversize.resize(MAX_ICON_BYTES + 1, 0);
    let response = send(upload("/api/services/example/icon", oversize))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(store.get("example").unwrap().unwrap().bytes(), PNG);

    let mut largest = PNG.to_vec();
    largest.resize(MAX_ICON_BYTES, 0);
    let response = send(upload("/api/services/example/icon", largest))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = send(upload("/api/services/missing/icon", PNG.to_vec()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(store.get("missing").unwrap(), None);
}

#[tokio::test]
async fn deleting_a_service_deletes_its_icon() {
    let store = icons();
    let app = router(
        Arc::new(FakeManager::new()),
        store.clone(),
        Shutdown::never(),
    );
    let response = app
        .clone()
        .oneshot(upload("/api/services/example/icon", PNG.to_vec()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .oneshot(request("DELETE", "/api/services/example", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(store.get("example").unwrap(), None);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn debug_builds_reload_open_pages_after_a_restart() {
    let app = router(Arc::new(FakeManager::new()), icons(), Shutdown::never());
    let page = app
        .clone()
        .oneshot(request("GET", "/", None))
        .await
        .unwrap();
    let html = page.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&html).contains("data-dev-reload"));
    let response = app
        .oneshot(request("GET", "/dev/reload", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let first = tokio::time::timeout(Duration::from_secs(2), response.into_body().frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(String::from_utf8_lossy(&first).contains("retry: 500"));
}

#[tokio::test]
async fn open_streams_end_when_shutdown_is_requested() {
    let (stop, shutdown) = Shutdown::channel();
    let app = router(Arc::new(FakeManager::new()), icons(), shutdown);
    let mut paths = vec!["/api/services/example/logs/stream"];
    if cfg!(debug_assertions) {
        paths.push("/dev/reload");
    }
    let mut bodies = vec![];
    for path in paths {
        let response = app
            .clone()
            .oneshot(request("GET", path, None))
            .await
            .unwrap();
        let mut body = response.into_body();
        tokio::time::timeout(Duration::from_secs(2), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        bodies.push((path, body));
    }
    stop.send(true).unwrap();
    for (path, mut body) in bodies {
        let end = tokio::time::timeout(Duration::from_secs(2), body.frame()).await;
        assert!(end.unwrap().is_none(), "{path} still open after shutdown");
    }
}

#[tokio::test]
async fn every_response_except_versioned_icons_is_never_cached() {
    let app = router(Arc::new(FakeManager::new()), icons(), Shutdown::never());
    let mut paths = vec![
        "/",
        "/services/example",
        "/static/app.js",
        "/api/services",
        "/api/services/example",
        "/api/services/example/status",
        "/api/services/example/logs",
        "/api/services/example/logs/stream",
        "/api/services/example/icon",
        "/nope",
    ];
    if cfg!(debug_assertions) {
        paths.push("/dev/reload");
    }
    for path in paths {
        let response = app
            .clone()
            .oneshot(request("GET", path, None))
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store", "{path}");
        let policy = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(policy.contains("frame-ancestors 'none'"), "{path}");
    }
}

#[tokio::test]
async fn icon_routes_answer_404_for_unknown_services() {
    let store = icons();
    store
        .put("missing", &Icon::parse(PNG.to_vec()).unwrap())
        .unwrap();
    let app = router(
        Arc::new(FakeManager::new()),
        store.clone(),
        Shutdown::never(),
    );
    for method in ["GET", "DELETE"] {
        let response = app
            .clone()
            .oneshot(request(method, "/api/services/missing/icon", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }
    assert!(store.get("missing").unwrap().is_some());
}
