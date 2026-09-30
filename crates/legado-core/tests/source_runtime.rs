use axum::{
    body::Body,
    http::{HeaderMap, Method, Response, Uri},
    routing::any,
    Router,
};
use reader_rust::{
    crawler::{
        fetcher::{fetch_with_client, RequestSpec},
        http_client::HttpClient,
        source_runtime::{SourceCookies, SourceRuntime, SourceSession},
    },
    model::{
        book_source::BookSource,
        rule::{ContentRule, SearchRule},
    },
    parser::{
        js::{eval_js, with_js_lib},
        rule_engine::RuleEngine,
    },
    service::book_service::BookService,
    storage::cache::file_cache::FileCache,
};
use reqwest::cookie::CookieStore;
use serde_json::{json, Value};
use std::{sync::Arc, thread};

// The server runs on its own OS thread so synchronous JavaScript HTTP also works
// when the caller uses a current-thread Tokio runtime.
struct Fixture {
    base: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
    storage: std::path::PathBuf,
}

impl Fixture {
    fn start() -> Self {
        let (address_tx, address_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let worker = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    address_tx.send(listener.local_addr().unwrap()).unwrap();
                    axum::serve(listener, Router::new().fallback(any(reply)))
                        .with_graceful_shutdown(async {
                            let _ = stop_rx.await;
                        })
                        .await
                        .unwrap();
                });
        });
        Self {
            base: format!("http://{}", address_rx.recv().unwrap()),
            stop: Some(stop_tx),
            worker: Some(worker),
            storage: std::env::temp_dir().join(format!("tlegado-runtime-{}", uuid::Uuid::new_v4())),
        }
    }
    fn service(&self) -> BookService {
        BookService::new(
            HttpClient::new(3, None).unwrap(),
            RuleEngine::new().unwrap(),
            FileCache::new(self.storage.join("cache")),
            self.storage.to_str().unwrap(),
        )
    }
    fn source(&self) -> BookSource {
        BookSource {
            book_source_url: self.base.clone(),
            book_source_name: "Local runtime".into(),
            enabled_cookie_jar: Some(true),
            search_url: Some("/search".into()),
            rule_search: Some(SearchRule {
                book_list: Some(".item".into()),
                name: Some(".title@text".into()),
                book_url: Some(".title@href".into()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(&self.storage);
    }
}
async fn reply(method: Method, uri: Uri, headers: HeaderMap, body: String) -> Response<Body> {
    let cookie = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");
    let token = headers
        .get("x-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");
    let mut response = Response::builder().header("X-Fixture", "present");
    match uri.path() {
        "/set" => {
            response = response.header("Set-Cookie", "native=present; Path=/");
        }
        "/set-js" => {
            response = response.header("Set-Cookie", "script=present; Path=/");
        }
        "/redirect" => {
            return response
                .status(302)
                .header("Location", "/echo")
                .body(Body::empty())
                .unwrap()
        }
        "/status" => return response.status(418).body(Body::from("teapot")).unwrap(),
        "/lib" => {
            return response
                .body(Body::from(format!("var libraryCookie={};", json!(cookie))))
                .unwrap()
        }
        "/gbk" => {
            return response
                .header("Content-Type", "text/plain; charset=gbk")
                .body(Body::from(vec![0xd6, 0xd0, 0xce, 0xc4]))
                .unwrap()
        }
        _ => {}
    }
    let body = if uri.path() == "/echo" {
        json!({"method":method.as_str(),"cookie":cookie,"token":token,"body":body}).to_string()
    } else if uri.path() == "/discovery-nav" {
        "<nav class='nav'><a href='/category?page={{page}}'>玄幻</a><a href='/complete'>完本</a></nav>".into()
    } else if uri.path() == "/discovery-denied" {
        return Response::builder()
            .status(403)
            .body(Body::from("denied"))
            .unwrap();
    } else if uri.path() == "/discovery-empty" {
        "<html></html>".into()
    } else if uri.path() == "/category" {
        format!(
            "<div class='item'><a class='title' href='/book'>{}</a></div>",
            uri.query().unwrap_or_default()
        )
    } else {
        format!("<div class='item'><a class='title' href='/book'>{cookie}</a></div>")
    };
    response.body(Body::from(body)).unwrap()
}
fn eval(runtime: &SourceRuntime, script: &str) -> String {
    runtime
        .enter(|| eval_js(script, "", &runtime.source.book_source_url))
        .unwrap()
}

#[tokio::test]
async fn discovery_jsoup_categories_load_and_fetch_paginated_books() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    source.explore_url = Some(r#"@JS:
        let nodes = org.jsoup.Jsoup.parse(java.ajax(source.getKey()+'/discovery-nav')).select('.nav a');
        let categories = [{title:'全部分类',url:''}];
        for (let i=0;i<nodes.size();i++) categories.push({title:nodes.get(i).text(),url:nodes[i].attr('href')});
        categories;
    "#.into());
    let kinds = service.explore_kinds(&source).unwrap();
    assert_eq!(kinds.len(), 3);
    assert_eq!(kinds[1].title, "玄幻");
    let books = service
        .explore_book("default", &source, kinds[1].url.as_deref().unwrap(), 2)
        .await
        .unwrap();
    assert_eq!(books[0].name, "page=2");
}

#[tokio::test]
async fn discovery_reports_http_and_first_page_parse_failures_but_allows_empty_later_pages() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    let error = service
        .explore_book("default", &source, "/discovery-denied", 1)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 403"));
    let error = service
        .explore_book("default", &source, "/discovery-empty", 1)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("未解析到书籍"));
    assert!(service
        .explore_book("default", &source, "/discovery-empty", 2)
        .await
        .unwrap()
        .is_empty());
}

#[test]
fn discovery_reports_invalid_json_and_script_errors_instead_of_fake_categories() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    for rule in [
        "[{broken]",
        "<js>1",
        "@js:throw Error('missing discovery API')",
    ] {
        source.explore_url = Some(rule.into());
        let error = service.explore_kinds(&source).unwrap_err().to_string();
        assert!(error.contains("发现分类"), "{error}");
    }
    source.explore_url = Some("<JS>[{title:'标题',url:''},{title:'分类',url:'/list'}]</JS>".into());
    assert_eq!(
        service.explore_kinds(&source).unwrap()[1].url.as_deref(),
        Some("/list")
    );
}

#[test]
fn discovery_jsoup_supports_nested_selectors_and_reports_unsupported_css() {
    let fixture = Fixture::start();
    let runtime = fixture
        .service()
        .source_runtime("default", &fixture.source());
    assert_eq!(
        eval(
            &runtime,
            r#"
        const doc = Packages.org.jsoup.Jsoup.parse('<nav><a href="/next">Hello &amp; World</a></nav>', 'https://example.test/base');
        const links=doc.select('nav').first().select('a');
        JSON.stringify([links.length,links.first().text(),links.attr('abs:href'),links.last().hasAttr('href'),links.isEmpty()]);
    "#
        ),
        r#"[1,"Hello & World","https://example.test/next",true,false]"#
    );
    let error = runtime
        .enter(|| {
            eval_js(
                "org.jsoup.Jsoup.parse('<div/>').select('a:unsupported')",
                "",
                &fixture.base,
            )
        })
        .unwrap_err();
    assert!(error.to_string().contains("Jsoup CSS selector"));
}

#[test]
fn discovery_can_resolve_a_category_that_is_a_book_detail_page() {
    let source = BookSource {
        book_source_url: "https://fixture.invalid".into(),
        rule_explore: Some(SearchRule {
            book_list: Some(".missing".into()),
            ..Default::default()
        }),
        rule_book_info: Some(reader_rust::model::rule::BookInfoRule {
            name: Some("h1@text".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let books = RuleEngine::new().unwrap().explore_books(
        &source,
        "<h1>分类直达详情</h1>",
        "https://fixture.invalid/book/1",
    );
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].name, "分类直达详情");
    assert_eq!(books[0].book_url, "https://fixture.invalid/book/1");
}

#[test]
fn cookie_store_preserves_scope_expiry_and_transactional_import() {
    let jar = SourceCookies::default();
    let url = "https://a.example.test/books/1".parse().unwrap();
    let headers = [
        "root=1; Path=/",
        "chapter=2; Path=/books",
        "secure=3; Secure; Path=/",
        "shared=4; Domain=example.test; Path=/",
        "expired=5; Max-Age=0; Path=/",
    ]
    .map(|s| s.parse().unwrap());
    jar.set_cookies(&mut headers.iter(), &url);
    let here = jar.get(url.as_str()).unwrap();
    for value in ["root=1", "chapter=2", "secure=3", "shared=4"] {
        assert!(here.contains(value));
    }
    assert!(!here.contains("expired"));
    let elsewhere = jar.get("http://a.example.test/other").unwrap();
    assert!(!elsewhere.contains("chapter") && !elsewhere.contains("secure"));
    assert_eq!(jar.get("https://b.example.test/").unwrap(), "shared=4");
    jar.put(url.as_str(), "root=new; manual=6", false).unwrap();
    assert!(jar.get(url.as_str()).unwrap().contains("chapter=2"));
    assert!(jar.put(url.as_str(), "root=lost; invalid", true).is_err());
    assert!(jar.get(url.as_str()).unwrap().contains("root=new"));
    jar.put("https://elsewhere.test/", "other=kept", true)
        .unwrap();
    jar.put(url.as_str(), "only=one", true).unwrap();
    assert_eq!(jar.get(url.as_str()).unwrap(), "only=one");
    jar.remove(url.as_str()).unwrap();
    assert!(jar.get(url.as_str()).unwrap().is_empty());
    assert_eq!(jar.get("https://elsewhere.test/").unwrap(), "other=kept");
}

#[test]
fn javascript_http_contracts_headers_redirects_charset_and_post() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let runtime = service.source_runtime("alice", &fixture.source());
    let observed: Value = serde_json::from_str(&eval(&runtime, r#"
        var a=java.ajax('/echo');
        var c=java.connect('/redirect');
        var g=java.get('/redirect',{});
        var p=java.post('/echo','a=1',{'X-Token':'explicit'});
        var s=java.connect('/status');
        JSON.stringify({ajaxType:typeof a,ajax:JSON.parse(a),code:c.code(),ok:c.isSuccessful(),
            finalUrl:c.url(),header:c.headers().get('x-FIXTURE'),redirect:g.statusCode(),
            location:g.header('location'),post:JSON.parse(p.body()),head:java.head('/echo',{}).body(),
            failed:s.code(),success:s.isSuccessful(),gbk:java.ajax('/gbk'),
            options:JSON.parse(java.ajax('/echo,'+JSON.stringify({headers:JSON.stringify({'X-Token':'options'})}))),
            serialized:JSON.parse(JSON.stringify(c)).code})
    "#)).unwrap();
    assert_eq!(observed["ajaxType"], "string");
    assert_eq!(observed["ajax"]["method"], "GET");
    assert_eq!(observed["code"], 200);
    assert_eq!(observed["serialized"], 200);
    assert_eq!(observed["ok"], true);
    assert_eq!(observed["finalUrl"], format!("{}/echo", fixture.base));
    assert_eq!(observed["header"], "present");
    assert_eq!(observed["redirect"], 302);
    assert_eq!(observed["location"], "/echo");
    assert_eq!(observed["post"]["method"], "POST");
    assert_eq!(observed["post"]["body"], "a=1");
    assert_eq!(observed["post"]["token"], "explicit");
    assert_eq!(observed["options"]["token"], "options");
    assert_eq!(observed["head"], "");
    assert_eq!(observed["failed"], 418);
    assert_eq!(observed["success"], false);
    assert_eq!(observed["gbk"], "中文");
}

#[tokio::test]
async fn native_search_and_javascript_share_cookies_in_both_directions() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    source.search_url = Some("/set".into());
    source.rule_search.as_mut().unwrap().book_list = Some(
        r#"@js:JSON.stringify([{name:JSON.parse(java.ajax('/echo')).cookie,bookUrl:'/book'}])"#
            .into(),
    );
    source.rule_search.as_mut().unwrap().name = Some("name".into());
    source.rule_search.as_mut().unwrap().book_url = Some("bookUrl".into());
    let books = service
        .search_book("alice", &source, "fixture", 1)
        .await
        .unwrap();
    assert_eq!(books[0].name, "native=present");
    let mut source = fixture.source();
    source.header = Some("@js:java.ajax('/set-js');JSON.stringify({})".into());
    let books = service
        .search_book("alice", &source, "fixture", 1)
        .await
        .unwrap();
    assert!(books[0].name.contains("script=present"));
    assert!(books[0].name.contains("native=present"));
    let runtime = service.source_runtime("alice", &source);
    eval(&runtime, "cookie.removeCookie(source.getKey());'ok'");
    source.header = None;
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "none"
    );
}

#[tokio::test]
async fn sessions_are_isolated_by_user_and_source_even_on_the_same_host() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    service
        .set_source_cookie("alice", &source.book_source_url, "session=alice")
        .await
        .unwrap();
    let mut second = source.clone();
    second.book_source_url = format!("{}/another-source", fixture.base);
    for (user, src, expected) in [
        ("alice", &source, "session=alice"),
        ("bob", &source, "none"),
        ("alice", &second, "none"),
    ] {
        assert_eq!(
            service.search_book(user, src, "fixture", 1).await.unwrap()[0].name,
            expected
        );
        let rt = service.source_runtime(user, src);
        let response: Value = serde_json::from_str(&eval(&rt, "java.ajax('/echo')")).unwrap();
        assert_eq!(response["cookie"], expected);
    }
}

#[tokio::test]
async fn disabled_cookie_jar_keeps_manual_cookies_but_ignores_response_cookies() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    source.enabled_cookie_jar = Some(false);
    source.search_url = Some("/set".into());
    service
        .search_book("alice", &source, "fixture", 1)
        .await
        .unwrap();
    let rt = service.source_runtime("alice", &source);
    eval(&rt, "java.ajax('/set-js')");
    assert!(rt.session.cookies.get(&fixture.base).unwrap().is_empty());
    eval(&rt, "cookie.setCookie(source.getKey(),'manual=yes');'ok'");
    source.search_url = Some("/search".into());
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "manual=yes"
    );
    let v: Value = serde_json::from_str(&eval(&rt, "java.ajax('/echo')")).unwrap();
    assert_eq!(v["cookie"], "manual=yes");
}

#[tokio::test]
async fn login_headers_have_consistent_precedence_and_do_not_resurrect_removed_cookies() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    source.header = Some(r#"{"X-Token":"source"}"#.into());
    let rt = service.source_runtime("alice", &source);
    eval(
        &rt,
        r#"source.putLoginHeader({Cookie:'login=ok','X-Token':'login'});'ok'"#,
    );
    assert_eq!(eval(&rt, "source.getLoginHeaderMap()['X-Token']"), "login");
    let v: Value = serde_json::from_str(&eval(&rt, "java.ajax('/echo')")).unwrap();
    assert_eq!(v["token"], "login");
    let v: Value = serde_json::from_str(&eval(
        &rt,
        "java.get('/echo',{'X-Token':'explicit'}).body()",
    ))
    .unwrap();
    assert_eq!(v["token"], "explicit");
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "login=ok"
    );
    eval(&rt, "cookie.removeCookie(source.getKey());'ok'");
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "none"
    );
    assert_eq!(eval(&rt, "source.getLoginHeaderMap().Cookie"), "");
    assert!(rt
        .enter(|| eval_js("source.putLoginHeader('{bad')", "", &fixture.base))
        .is_err());
    assert_eq!(eval(&rt, "source.getLoginHeaderMap()['X-Token']"), "login");
    eval(&rt, "source.removeLoginHeader();'ok'");
    assert_eq!(eval(&rt, "source.getLoginHeaderMap() === null"), "true");
}

#[test]
fn source_state_and_operation_variables_have_distinct_lifetimes() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    let first = service.source_runtime("alice", &source);
    assert_eq!(eval(&first, "java.put('v','first');source.setVariable('saved');cache.put('x','cached');java.get('v')"), "first");
    assert_eq!(eval(&first, "java.get('v')"), "first");
    let second = service.source_runtime("alice", &source);
    assert_eq!(
        eval(
            &second,
            "JSON.stringify([java.get('v'),source.getVariable(),cache.get('x')])"
        ),
        r#"["","saved","cached"]"#
    );
    let other = service.source_runtime("bob", &source);
    assert_eq!(eval(&other, "source.getVariable()"), "");
}

#[test]
fn downloaded_libraries_cannot_share_authenticated_content_between_users() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    let lib = json!({"fixture":format!("{}/lib",fixture.base)}).to_string();
    for user in ["alice", "bob", "alice"] {
        let runtime = service.source_runtime(user, &source);
        runtime
            .session
            .cookies
            .put(&fixture.base, &format!("user={user}"), true)
            .unwrap();
        let result = runtime
            .enter(|| with_js_lib(Some(&lib), || eval_js("libraryCookie", "", &fixture.base)))
            .unwrap();
        assert_eq!(result, format!("user={user}"));
    }
    let runtime = service.source_runtime("alice", &source);
    eval(
        &runtime,
        "cookie.setCookie(source.getKey(),'user=changed');'ok'",
    );
    let result = runtime
        .enter(|| with_js_lib(Some(&lib), || eval_js("libraryCookie", "", &fixture.base)))
        .unwrap();
    assert_eq!(result, "user=changed");
}

#[test]
fn parser_source_identity_is_independent_of_book_and_page_urls() {
    let source = BookSource {
        book_source_url: "https://source.test".into(),
        rule_content: Some(ContentRule {
            content: Some(
                "@js:JSON.stringify([source.getKey(),source.bookSourceUrl,book.bookUrl,baseUrl])"
                    .into(),
            ),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        RuleEngine::new().unwrap().content(
            &source,
            "",
            "https://content.test/chapter",
            Some("https://book.test/123")
        ),
        r#"["https://source.test","https://source.test","https://book.test/123","https://content.test/chapter"]"#
    );
}

#[tokio::test]
async fn task_contexts_survive_interleaved_awaits_and_restore_after_cancellation() {
    let http = HttpClient::new(3, None).unwrap();
    let a = SourceRuntime::new(
        BookSource {
            book_source_url: "https://a.test".into(),
            ..Default::default()
        },
        Arc::new(SourceSession::default()),
        http.clone(),
    );
    let b = SourceRuntime::new(
        BookSource {
            book_source_url: "https://b.test".into(),
            ..Default::default()
        },
        Arc::new(SourceSession::default()),
        http,
    );
    let (av, bv) = tokio::join!(
        a.scope(async {
            tokio::task::yield_now().await;
            eval_js("source.getKey()", "", "").unwrap()
        }),
        b.scope(async {
            tokio::task::yield_now().await;
            eval_js("source.getKey()", "", "").unwrap()
        })
    );
    assert_eq!(av, "https://a.test");
    assert_eq!(bv, "https://b.test");
    assert!(SourceRuntime::current().is_none());
    let task = tokio::spawn(async move {
        a.scope(std::future::pending::<()>()).await;
    });
    tokio::task::yield_now().await;
    task.abort();
    let _ = task.await;
    assert!(SourceRuntime::current().is_none());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.enter(|| panic!("fixture"))
    }));
    assert!(panic.is_err());
    assert!(SourceRuntime::current().is_none());
}

#[tokio::test]
async fn login_check_receives_response_methods_and_propagates_failure() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let mut source = fixture.source();
    source.login_check_js = Some(
        "if(result.code()!==200||!result.body().includes('item')) throw Error('unexpected');result"
            .into(),
    );
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()
            .len(),
        1
    );
    for script in ["throw Error('expired')", "false"] {
        source.login_check_js = Some(script.into());
        let error = service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("loginCheckJs"));
    }
    source.login_url = Some("/status".into());
    source.login_check_js = None;
    assert_eq!(
        service.login_book_source(&source).await.unwrap()["success"],
        false
    );
}

#[tokio::test]
async fn browser_requests_report_unsupported_in_native_and_script_paths() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let error = fetch_with_client(
        &service.http_client(),
        RequestSpec {
            url: format!("{}/echo", fixture.base),
            web_view: true,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("unsupported capability"));
    let rt = service.source_runtime("alice", &fixture.source());
    assert_eq!(
        eval(
            &rt,
            r#"try { java.ajax('/echo,{"webView":true}'); 'unexpected'; } catch(e) { e.message; }"#
        ),
        "unsupported capability: WebView/browser JavaScript"
    );
}

fn session_file(fixture: &Fixture, user: &str) -> std::path::PathBuf {
    std::fs::read_dir(
        fixture
            .storage
            .join("data")
            .join(user)
            .join("source_sessions"),
    )
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .find(|path| path.extension().is_some_and(|ext| ext == "json"))
    .unwrap()
}

#[tokio::test]
async fn persistent_sessions_restore_native_and_js_auth_without_restoring_operation_scratch() {
    let fixture = Fixture::start();
    let mut source = fixture.source();
    {
        let service = fixture.service();
        source.search_url = Some("/set".into());
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap();
        let runtime = service.source_runtime("alice", &source);
        eval(
            &runtime,
            r#"java.ajax('/set-js');source.setVariable('saved');cache.put('cache','value');source.put('setting','theme');source.putLoginHeader({'X-Token':'restored'});java.put('scratch','temporary');'ok'"#,
        );
        runtime
            .session
            .js_lib_cache
            .lock()
            .unwrap()
            .insert("downloaded".into(), "memory only".into());
    }
    let service = fixture.service();
    source.search_url = Some("/search".into());
    let books = service
        .search_book("alice", &source, "fixture", 1)
        .await
        .unwrap();
    assert!(books[0].name.contains("native=present") && books[0].name.contains("script=present"));
    let runtime = service.source_runtime("alice", &source);
    let response: Value = serde_json::from_str(&eval(&runtime, "java.ajax('/echo')")).unwrap();
    assert_eq!(response["token"], "restored");
    assert_eq!(eval(&runtime,"JSON.stringify([source.getVariable(),cache.get('cache'),source.get('setting'),java.get('scratch')])"),r#"["saved","value","theme",""]"#);
    assert!(runtime.session.js_lib_cache.lock().unwrap().is_empty());
    assert_eq!(
        service
            .search_book("bob", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "none"
    );
    let mut other = source.clone();
    other.book_source_url.push_str("/different");
    assert_eq!(
        service
            .search_book("alice", &other, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "none"
    );
}

#[tokio::test]
async fn cleared_and_deleted_cookies_stay_cleared_after_restart() {
    let fixture = Fixture::start();
    let source = fixture.source();
    {
        let service = fixture.service();
        let runtime = service.source_runtime("alice", &source);
        eval(&runtime,"source.putLoginHeader({Cookie:'login=old','X-Token':'old'});cookie.removeCookie(source.getKey());'ok'");
        let snapshot = std::fs::read_to_string(session_file(&fixture, "alice")).unwrap();
        assert!(!snapshot.contains("login=old"));
        let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(snapshot["login_headers"]["Cookie"], "");
    }
    {
        let service = fixture.service();
        assert_eq!(
            service
                .search_book("alice", &source, "fixture", 1)
                .await
                .unwrap()[0]
                .name,
            "none"
        );
        let runtime = service.source_runtime("alice", &source);
        assert_eq!(eval(&runtime, "source.getLoginHeaderMap().Cookie"), "");
        eval(
            &runtime,
            "cookie.replaceCookie(source.getKey(),'login=new');'ok'",
        );
        service
            .clear_source_cookie("alice", &source.book_source_url)
            .await
            .unwrap();
    }
    let service = fixture.service();
    let runtime = service.source_runtime("alice", &source);
    assert_eq!(
        service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "none"
    );
    assert_eq!(eval(&runtime, "source.getLoginHeaderMap()===null"), "true");
}

#[test]
fn persistent_cookie_snapshots_keep_session_cookies_paths_domains_secure_and_expiry() {
    let fixture = Fixture::start();
    let url = "https://a.example.test/books/1";
    {
        let session = SourceSession::persistent(&fixture.storage, "alice", url);
        let headers = [
            "session=kept; Path=/",
            "chapter=kept; Path=/books",
            "secure=kept; Secure; Path=/",
            "shared=kept; Domain=example.test; Path=/",
            "expired=gone; Max-Age=0; Path=/",
        ]
        .map(|v| v.parse().unwrap());
        session
            .cookies
            .set_cookies(&mut headers.iter(), &url.parse().unwrap());
        session.check().unwrap();
    }
    let session = SourceSession::persistent(&fixture.storage, "alice", url);
    session.check().unwrap();
    let text = session.cookies.get(url).unwrap();
    for name in ["session=kept", "chapter=kept", "secure=kept", "shared=kept"] {
        assert!(text.contains(name));
    }
    assert!(!text.contains("expired"));
    let text = session.cookies.get("http://a.example.test/other").unwrap();
    assert!(!text.contains("chapter") && !text.contains("secure"));
    assert_eq!(
        session.cookies.get("https://b.example.test/").unwrap(),
        "shared=kept"
    );
    // Simulate a persistent cookie that expires while the application is closed.
    let path = session_file(&fixture, "alice");
    let mut snapshot: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for cookie in snapshot["cookies"].as_array_mut().unwrap() {
        cookie["expires"] = json!({"AtUtc":"2000-01-01T00:00:00Z"});
    }
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let expired = SourceSession::persistent(&fixture.storage, "alice", url);
    expired.check().unwrap();
    assert!(expired.cookies.get(url).unwrap().is_empty());
}

#[tokio::test]
async fn corrupt_or_mismatched_session_files_are_reported_without_overwrite() {
    let fixture = Fixture::start();
    let source = fixture.source();
    {
        fixture
            .service()
            .set_source_cookie("alice", &source.book_source_url, "original=kept")
            .await
            .unwrap();
    }
    let path = session_file(&fixture, "alice");
    let original = std::fs::read(&path).unwrap();
    let mut wrong_version: Value = serde_json::from_slice(&original).unwrap();
    wrong_version["version"] = json!(999);
    let mut wrong_identity: Value = serde_json::from_slice(&original).unwrap();
    wrong_identity["identity"] = json!("different owner");
    for bytes in [
        b"broken json".to_vec(),
        serde_json::to_vec(&wrong_version).unwrap(),
        serde_json::to_vec(&wrong_identity).unwrap(),
    ] {
        std::fs::write(&path, &bytes).unwrap();
        let service = fixture.service();
        let error = service
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("source session could not be loaded"));
        assert!(service
            .set_source_cookie("alice", &source.book_source_url, "replace=no")
            .await
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::write(&path, original).unwrap();
    assert_eq!(
        fixture
            .service()
            .search_book("alice", &source, "fixture", 1)
            .await
            .unwrap()[0]
            .name,
        "original=kept"
    );
}

#[test]
fn concurrent_updates_commit_one_complete_snapshot_without_losing_keys() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    let session = service.source_runtime("alice", &source).session;
    std::thread::scope(|scope| {
        for i in 0..16 {
            let session = session.clone();
            let url = fixture.base.clone();
            scope.spawn(move || {
                session
                    .cache_put(format!("key{i}"), format!("value{i}"))
                    .unwrap();
                session
                    .cookies
                    .put(&url, &format!("cookie{i}=present"), false)
                    .unwrap();
            });
        }
    });
    let restored = fixture.service().source_runtime("alice", &source).session;
    for i in 0..16 {
        assert_eq!(
            restored.cache_get(&format!("key{i}")),
            Some(format!("value{i}"))
        );
    }
    assert_eq!(
        restored
            .cookies
            .get(&fixture.base)
            .unwrap()
            .split(';')
            .count(),
        16
    );
    let path = session_file(&fixture, "alice");
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
}

#[tokio::test]
async fn disk_failures_rollback_js_mutations_and_reach_http_callers() {
    let fixture = Fixture::start();
    let service = fixture.service();
    let source = fixture.source();
    let runtime = service.source_runtime("alice", &source);
    // Make the intended parent a file so writes deterministically fail on all OSes.
    std::fs::create_dir_all(&fixture.storage).unwrap();
    let blocked = fixture.storage.join("data");
    std::fs::write(&blocked, b"preserve me").unwrap();
    let error = runtime
        .enter(|| eval_js("source.setVariable('unsaved')", "", &fixture.base))
        .unwrap_err()
        .to_string();
    assert!(error.contains("could not persist source session"));
    assert_eq!(runtime.session.variable(), "");
    assert!(service
        .set_source_cookie("alice", &source.book_source_url, "unsaved=bad")
        .await
        .is_err());
    assert!(runtime
        .session
        .cookies
        .get(&fixture.base)
        .unwrap()
        .is_empty());
    let js_error = eval(
        &runtime,
        "try { java.ajax('/set-js'); 'unexpected'; } catch(e) {e.message;}",
    );
    assert!(js_error.contains("persist response cookies"));
    let mut setting = source.clone();
    setting.search_url = Some("/set".into());
    let error = service
        .search_book("alice", &setting, "fixture", 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("persist response cookies"));
    assert_eq!(std::fs::read(&blocked).unwrap(), b"preserve me");
    std::fs::remove_file(blocked).unwrap();
    eval(&runtime, "source.setVariable('saved');'ok'");
    assert_eq!(
        fixture
            .service()
            .source_runtime("alice", &source)
            .session
            .variable(),
        "saved"
    );
}

#[test]
fn namespace_traversal_is_rejected_and_source_urls_never_form_paths() {
    let fixture = Fixture::start();
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "..\\escape",
        "C:\\escape",
        "alice/child",
    ] {
        let session = SourceSession::persistent(&fixture.storage, name, "https://source.test/");
        assert!(session.check().is_err());
        assert!(session.set_variable("no".into()).is_err());
    }
    let session = SourceSession::persistent(
        &fixture.storage,
        "alice",
        "https://source.test/a/../../x?secret=token",
    );
    session.set_variable("yes".into()).unwrap();
    let path = session_file(&fixture, "alice");
    assert_eq!(path.file_stem().unwrap().to_string_lossy().len(), 64);
    assert!(!path.to_string_lossy().contains("secret"));
}

#[cfg(windows)]
#[test]
fn denied_atomic_replacement_preserves_existing_snapshot_and_cleans_temporary_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::start();
    let source = fixture.source();
    let service = fixture.service();
    let session = service.source_runtime("alice", &source).session;
    session.set_variable("original".into()).unwrap();
    let path = session_file(&fixture, "alice");
    let before = std::fs::read(&path).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(session.set_variable("replacement".into()).is_err());
    assert_eq!(session.variable(), "original");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    drop(held);
    session.set_variable("replacement".into()).unwrap();
    assert_eq!(
        fixture
            .service()
            .source_runtime("alice", &source)
            .session
            .variable(),
        "replacement"
    );
}
