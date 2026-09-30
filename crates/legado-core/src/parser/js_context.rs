use crate::crawler::source_runtime::SourceRuntime;
use rquickjs::{function::Func, Ctx, Object};
use serde_json::json;

fn status(result: anyhow::Result<()>) -> String {
    result.err().map(|e| e.to_string()).unwrap_or_default()
}

pub(super) fn install<'js>(
    ctx: Ctx<'js>,
    runtime: &SourceRuntime,
    base: &str,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let source: Object = ctx
        .json_parse(serde_json::to_string(&runtime.source).unwrap())?
        .into_object()
        .unwrap();
    let key = runtime.source.book_source_url.clone();
    source.set("key", key.clone())?;
    source.set("getKey", Func::new(move || key.clone()))?;
    let session = runtime.session.clone();
    source.set("getVariable", Func::new(move || session.variable()))?;
    let session = runtime.session.clone();
    source.set(
        "__setVariable",
        Func::new(move |value: String| status(session.set_variable(value))),
    )?;
    let session = runtime.session.clone();
    source.set(
        "get",
        Func::new(move |key: String| session.cache_get(&key).unwrap_or_default()),
    )?;
    let session = runtime.session.clone();
    source.set(
        "__put",
        Func::new(move |key: String, value: String| status(session.cache_put(key, value))),
    )?;
    let session = runtime.session.clone();
    let source_url = runtime.source.book_source_url.clone();
    source.set(
        "getLoginHeader",
        Func::new(move || {
            let mut headers = session.login_headers();
            if headers.is_empty() {
                return None;
            }
            for (key, value) in &mut headers {
                if key.eq_ignore_ascii_case("cookie") {
                    *value = session.cookies.get(&source_url).unwrap_or_default();
                }
            }
            Some(serde_json::to_string(&headers).unwrap())
        }),
    )?;
    let session = runtime.session.clone();
    let source_url = runtime.source.book_source_url.clone();
    source.set(
        "__putLoginHeader",
        Func::new(move |text: String| status(session.put_login_headers(&source_url, &text))),
    )?;
    let session = runtime.session.clone();
    let source_url = runtime.source.book_source_url.clone();
    source.set(
        "__removeLoginHeader",
        Func::new(move || status(session.remove_login_headers(&source_url))),
    )?;
    globals.set("source", source)?;
    ctx.eval::<(),_>(r#"
        source.setVariable = function(v) { const e=source.__setVariable(String(v)); if(e)throw Error(e); };
        source.put = function(k,v) { const e=source.__put(String(k),String(v)); if(e)throw Error(e); return String(v); };
        source.getLoginHeaderMap = function() { const h=source.getLoginHeader(); return h?JSON.parse(h):null; };
        source.putLoginHeader = function(v) { const e=source.__putLoginHeader(typeof v==='string'?v:JSON.stringify(v)); if(e)throw Error(e); };
        source.removeLoginHeader = function() { const e=source.__removeLoginHeader(); if(e)throw Error(e); };
    "#)?;
    let cookie = Object::new(ctx.clone())?;
    let jar = runtime.session.cookies.clone();
    cookie.set(
        "getCookie",
        Func::new(move |url: String| jar.get(&url).unwrap_or_default()),
    )?;
    let session = runtime.session.clone();
    cookie.set(
        "__remove",
        Func::new(move |url: String| {
            let result = session.cookies.remove(&url);
            if result.is_ok() {
                session
                    .js_lib_cache
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
            }
            status(result)
        }),
    )?;
    let session = runtime.session.clone();
    cookie.set(
        "__set",
        Func::new(move |url: String, header: String, replace: bool| {
            let result = session.cookies.put(&url, &header, replace);
            if result.is_ok() {
                session
                    .js_lib_cache
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
            }
            status(result)
        }),
    )?;
    globals.set("cookie", cookie)?;
    ctx.eval::<(),_>(r#"
        cookie.setCookie = function(u,v) { const e=cookie.__set(String(u),v==null?'':String(v),true); if(e)throw Error(e); };
        cookie.replaceCookie = function(u,v) { const e=cookie.__set(String(u),String(v),false); if(e)throw Error(e); };
        cookie.removeCookie = function(u) { const e=cookie.__remove(String(u)); if(e)throw Error(e); };
    "#)?;
    let cache = Object::new(ctx.clone())?;
    let session = runtime.session.clone();
    cache.set("get", Func::new(move |key: String| session.cache_get(&key)))?;
    let session = runtime.session.clone();
    cache.set(
        "__put",
        Func::new(move |key: String, value: String| status(session.cache_put(key, value))),
    )?;
    globals.set("cache", cache)?;
    ctx.eval::<(),_>("cache.put=function(k,v){const e=cache.__put(String(k),String(v));if(e)throw Error(e);return true;};")?;
    let vars = runtime.variables.clone();
    globals.set(
        "__ruleGet",
        Func::new(move |key: String| {
            vars.lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&key)
                .cloned()
                .unwrap_or_default()
        }),
    )?;
    let vars = runtime.variables.clone();
    globals.set(
        "__rulePut",
        Func::new(move |key: String, val: String| {
            vars.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key, val);
        }),
    )?;
    let runtime = runtime.clone();
    let base = if base.is_empty() {
        runtime.source.book_source_url.clone()
    } else {
        base.to_string()
    };
    globals.set(
        "__sourceRequest",
        Func::new(
            move |url: String, method: String, body: Option<String>, headers: String| {
                match super::js_http::request(&runtime, &base, &url, &method, body, &headers) {
                    Ok(response) => json!({"ok":true,"response":response}).to_string(),
                    Err(error) => json!({"ok":false,"error":error.to_string()}).to_string(),
                }
            },
        ),
    )?;
    Ok(())
}
