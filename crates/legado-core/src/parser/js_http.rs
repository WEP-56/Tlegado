use crate::crawler::{
    fetcher::{decode_body, HttpMethod, StrResponse},
    source_runtime::SourceRuntime,
    url_analyzer::{analyze_url, parse_source_headers},
};

pub(super) fn request(
    runtime: &SourceRuntime,
    base: &str,
    spec: &str,
    method: &str,
    body: Option<String>,
    headers: &str,
) -> anyhow::Result<StrResponse> {
    let mut source = runtime.source.clone();
    if crate::crawler::source_runtime::in_header() {
        source.header = None;
    }
    let mut req = analyze_url(spec, "", 1, base, &source)
        .map_err(|_| anyhow::anyhow!("invalid JavaScript request specification"))?;
    anyhow::ensure!(
        !req.web_view && req.web_js.as_deref().unwrap_or_default().trim().is_empty(),
        "unsupported capability: WebView/browser JavaScript"
    );
    for (key, value) in parse_source_headers(headers) {
        req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(&key));
        req.headers.push((key, value));
    }
    // AnalyzeUrl (ajax/connect) follows redirects; Jsoup get/post/head does not.
    let follow_redirects = method.is_empty();
    let method = if method.is_empty() {
        match req.method {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
        }
    } else {
        method
    }
    .to_string();
    if let Some(body) = body {
        req.body = Some(body);
    }
    let runtime = runtime.clone();
    super::js::blocking_http(move || {
        let client = runtime.http.blocking_client_with_cookies(
            req.proxy.as_deref(),
            runtime.cookies(),
            follow_redirects,
        )?;
        for attempt in 0..=req.retry {
            let mut builder =
                client.request(reqwest::Method::from_bytes(method.as_bytes())?, &req.url);
            for (key, value) in &req.headers {
                builder = builder.header(key, value);
            }
            if let Some(body) = &req.body {
                if !req
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                {
                    builder = builder.header("Content-Type", "application/x-www-form-urlencoded");
                }
                builder = builder.body(body.clone());
            }
            match builder.send() {
                Ok(response) => {
                    runtime.session.check()?;
                    let code = response.status().as_u16();
                    if code >= 500 && attempt < req.retry {
                        std::thread::sleep(std::time::Duration::from_millis(
                            200 * (attempt as u64 + 1),
                        ));
                        continue;
                    }
                    let url = response.url().to_string();
                    let is_successful = response.status().is_success();
                    let content_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let headers = response
                        .headers()
                        .iter()
                        .filter_map(|(k, v)| {
                            v.to_str().ok().map(|v| (k.to_string(), v.to_string()))
                        })
                        .collect();
                    let bytes = response
                        .bytes()
                        .map_err(|_| anyhow::anyhow!("JavaScript HTTP body read failed"))?;
                    let body = if req.response_type.as_deref().is_some_and(|s| !s.is_empty()) {
                        hex::encode(bytes)
                    } else {
                        decode_body(&bytes, req.charset.as_deref(), content_type.as_deref())
                    };
                    return Ok(StrResponse {
                        body,
                        url,
                        code,
                        headers,
                        is_successful,
                    });
                }
                Err(_) if attempt < req.retry => {
                    std::thread::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1)))
                }
                // Do not leak URLs (which can contain credentials) into logs or UI errors.
                Err(_) => anyhow::bail!("JavaScript HTTP request failed"),
            }
        }
        anyhow::bail!("JavaScript HTTP retry limit exceeded")
    })
}
