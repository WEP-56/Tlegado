//! Run without libtest output capture so raw writes cannot hide a TUI regression.
use axum::{http::StatusCode, routing::get, Router};
use reader_rust::crawler::fetcher::{fetch_with_client, HttpMethod, RequestSpec};
use std::{process::Command, time::Duration};

#[test]
fn fetch_does_not_write_to_terminal() {
    const CHILD: &str = "TLEGADO_FETCH_OUTPUT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fetch_does_not_write_to_terminal", "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "child failed: {stdout}\n{stderr}");
        assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
        for marker in ["DEBUG: fetch", "/terminal-probe", "terminal-body-marker"] {
            assert!(!stdout.contains(marker), "unexpected stdout: {stdout}");
        }
        return;
    }

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let router = Router::new()
            .route(
                "/terminal-probe",
                get(|| async { "ok" }).post(|| async { "ok" }),
            )
            .route(
                "/failure",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        for method in [HttpMethod::GET, HttpMethod::POST] {
            let response = fetch_with_client(
                &client,
                RequestSpec {
                    url: format!("{base}/terminal-probe"),
                    body: (method == HttpMethod::POST).then(|| "terminal-body-marker".into()),
                    method,
                    retry: 0,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(response.body, "ok");
        }
        let response = fetch_with_client(
            &client,
            RequestSpec {
                url: format!("{base}/failure"),
                retry: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, 500);
        server.abort();
        let _ = server.await;
    });
}
