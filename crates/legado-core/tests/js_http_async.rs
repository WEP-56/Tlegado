//! A fresh subprocess catches both the first-use Lazy panic and terminal writes.
use reader_rust::parser::js::{eval_js, with_js_lib};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    time::Duration,
};

#[test]
fn js_http_is_safe_from_async_tasks_and_silent() {
    const CHILD: &str = "TLEGADO_JS_HTTP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        for flavor in ["current", "multi"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "js_http_is_safe_from_async_tasks_and_silent",
                    "--nocapture",
                ])
                .env(CHILD, flavor)
                .env("NO_PROXY", "127.0.0.1,localhost")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{flavor}: {stdout}\n{stderr}");
            assert!(stderr.is_empty(), "unexpected terminal output: {stderr}");
            assert!(!stdout.contains('\x1b') && !stdout.contains("panicked"));
        }
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut served = 0;
        while served < 4 && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 8192];
                    let n = stream.read(&mut request).unwrap();
                    let request = String::from_utf8_lossy(&request[..n]);
                    let body = if request.starts_with("GET /lib") {
                        "var fixtureValue='library-ok';"
                    } else {
                        "http-ok"
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                    served += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("{e}"),
            }
        }
        assert_eq!(served, 4);
    });
    let mut builder = if std::env::var(CHILD).unwrap() == "multi" {
        let mut b = tokio::runtime::Builder::new_multi_thread();
        b.worker_threads(2);
        b
    } else {
        tokio::runtime::Builder::new_current_thread()
    };
    let runtime = builder.enable_all().build().unwrap();
    runtime.block_on(async {
        for method in ["ajax", "get", "ajax"] {
            assert_eq!(
                eval_js(&format!("java.{method}('{base}/data')"), "", &base).unwrap(),
                "http-ok"
            );
        }
        let lib = serde_json::json!({"fixture": format!("{base}/lib")}).to_string();
        assert_eq!(
            with_js_lib(Some(&lib), || eval_js("fixtureValue", "", &base)).unwrap(),
            "library-ok"
        );
    });
    drop(runtime);
    server.join().unwrap();
}
