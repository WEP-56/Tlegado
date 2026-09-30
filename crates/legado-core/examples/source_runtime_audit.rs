//! Diagnostic observations, not assertions that current incompatibilities are correct.
//! Only the built-in loopback server receives HTTP requests. No source scripts run.
use reader_rust::parser::js::{eval_js, with_source_key};
use reader_rust::{
    crawler::{
        http_client::HttpClient,
        source_runtime::{SourceRuntime, SourceSession},
    },
    model::book_source::BookSource,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct Fixture {
    base: String,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn start() -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let base = format!("http://{}", listener.local_addr()?);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("fixture accept failed: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 2048];
                while bytes.len() < 16384 {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => bytes.extend_from_slice(&buf[..n]),
                    }
                    if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                let route = request.lines().next().unwrap_or_default();
                let body = if route.contains("/cookie") {
                    if request.contains("audit_session=present") {
                        "cookie-present"
                    } else {
                        "cookie-absent"
                    }
                } else if route.contains("/header") {
                    if request.contains("x-audit: present") {
                        "header-present"
                    } else {
                        "header-absent"
                    }
                } else {
                    "fixture-ok"
                };
                let extra = if route.contains("/set") {
                    "Set-Cookie: audit_session=present; Path=/\r\n"
                } else {
                    ""
                };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}", body.len());
            }
        });
        Ok(Self {
            base,
            stopped,
            worker: Some(worker),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn evaluate(script: &str, base: &str) -> Value {
    match eval_js(script, "<div>fixture</div>", base) {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":format!("{error:?}")}),
    }
}

fn main() -> anyhow::Result<()> {
    // Never send loopback requests through an environment-configured proxy.
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    std::env::set_var("no_proxy", "127.0.0.1,localhost");
    let fixture = Fixture::start()?;
    let context = SourceRuntime::new(
        BookSource {
            book_source_url: fixture.base.clone(),
            enabled_cookie_jar: Some(true),
            ..Default::default()
        },
        Arc::new(SourceSession::default()),
        HttpClient::new(3, None)?,
    );
    context.enter(|| run_audit(&fixture, &context))
}

fn run_audit(fixture: &Fixture, context: &SourceRuntime) -> anyhow::Result<()> {
    let base = &fixture.base;
    let mut observations = Vec::new();

    // Mirror normal crawler's caller-owned async client, before JS sets cookies.
    let runtime = tokio::runtime::Runtime::new()?;
    let normal_cookie = runtime.block_on(async {
        let client = context.http.client_with_cookies(None, context.cookies())?;
        client
            .get(format!("{base}/set"))
            .send()
            .await?
            .text()
            .await?;
        let body = client
            .get(format!("{base}/cookie"))
            .send()
            .await?
            .text()
            .await?;
        Ok::<_, anyhow::Error>(body)
    })?;
    observations.push(
        json!({"probe":"normal_http_cookie_vs_js_http", "normal_client":normal_cookie,
        "js_client":evaluate(&format!("java.ajax('{base}/cookie')"),base),
        "target":"The same source session must be available through both request paths."}),
    );

    let web_response = runtime.block_on(async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()?;
        reader_rust::crawler::fetcher::fetch_with_client(
            &client,
            reader_rust::crawler::fetcher::RequestSpec {
                url: format!("{base}/page"),
                web_view: true,
                web_js: Some("'browser-result'".to_string()),
                ..Default::default()
            },
        )
        .await
    });
    let web_response = match web_response {
        Ok(response) => json!({"ok":true,"body":response.body}),
        Err(error) => json!({"ok":false,"error":error.to_string()}),
    };
    observations.push(json!({"probe":"webview_request_dispatch", "observed":web_response,
        "target":"Execute browser JS or report an explicit unsupported capability instead of silent ordinary HTTP."}));

    observations.push(json!({"probe":"rule_variable_roundtrip", "expected":"stored-value",
        "observed":evaluate("java.put('audit-variable','stored-value');java.get('audit-variable')",base)}));
    observations.push(json!({"probe":"get_string_binding", "expected":"function",
        "observed":evaluate("typeof java.getString",base)}));
    observations.push(json!({"probe":"source_get_key_in_content_scope", "expected":"source identity, not book identity",
        "observed":with_source_key(Some("https://fixture.invalid/book/123"),||eval_js("source.getKey()","",base)).ok()}));

    // AnalyzeUrl.UrlOption uses plural headers; singular header is a negative control.
    for (name, options, expected) in [
        (
            "singular_header_control",
            json!({"header":{"X-Audit":"present"}}),
            "header-absent",
        ),
        (
            "headers_object",
            json!({"headers":{"X-Audit":"present"}}),
            "header-present",
        ),
        (
            "headers_json_string",
            json!({"headers":"{\"X-Audit\":\"present\"}"}),
            "header-present",
        ),
    ] {
        let spec = format!("{base}/header,{options}");
        observations.push(json!({"probe":format!("ajax_{name}"),"expected":expected,
            "observed":evaluate(&format!("java.ajax({})",serde_json::to_string(&spec)?),base)}));
    }
    observations.push(json!({"probe":"http_get_overload_and_response", "expected":"response object with body() and supplied request headers",
        "observed":evaluate(&format!("var r=java.get('{base}/header',{{'X-Audit':'present'}});JSON.stringify({{kind:typeof r,bodyMethod:typeof r.body,body:typeof r==='string'?r:r.body()}})"),base)}));
    observations.push(
        json!({"probe":"post_response_body_method", "expected":"fixture-ok",
        "observed":evaluate(&format!("java.post('{base}/post','data',{{}}).body()"),base)}),
    );

    eval_js(&format!("java.ajax('{base}/set')"), "", base)?;
    observations.push(json!({"probe":"cookie_get_after_http_set_cookie", "expected":"cookie includes audit_session",
        "observed":evaluate(&format!("cookie.getCookie('{base}')"),base)}));
    observations.push(json!({"probe":"cookie_remove_affects_http", "expected":"cookie-absent",
        "observed":evaluate(&format!("cookie.removeCookie('{base}');java.ajax('{base}/cookie')"),base)}));
    eval_js(&format!("java.ajax('{base}/set')"), "", base)?;
    let other = SourceRuntime::new(
        BookSource {
            book_source_url: "https://different-source.invalid".into(),
            enabled_cookie_jar: Some(true),
            ..Default::default()
        },
        Arc::new(SourceSession::default()),
        context.http.clone(),
    );
    observations.push(json!({"probe":"source_b_sees_source_a_http_cookie", "expected":"cookie-absent under Tlegado source isolation",
        "observed":other.enter(||evaluate(&format!("java.ajax('{base}/cookie')"),base)),
        "originalSourceStillHasCookie":evaluate(&format!("java.ajax('{base}/cookie')"),base)}));
    observations.push(json!({"probe":"missing_login_and_browser_bindings", "observed":evaluate(
        "JSON.stringify({loginInfo:typeof source.getLoginInfoMap,loginHeader:typeof source.putLoginHeader,sourceVariable:typeof source.getVariable,browser:typeof java.startBrowserAwait,webView:typeof java.webView,connect:typeof java.connect})",base)}));
    let output = serde_json::to_string_pretty(
        &json!({"scope":"synthetic loopback probes; expected values from local Legado reference; not a live-source pass rate", "observations":observations}),
    )?;
    if let Some(filename) = std::env::args_os().nth(1) {
        std::fs::write(filename, format!("{output}\n"))?;
    }
    println!("{output}");
    Ok(())
}
