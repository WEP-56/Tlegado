//! Read-only HTML operations used by common discovery scripts.
use rquickjs::{function::Func, Ctx};
use scraper::{Html, Selector};
use serde_json::json;

pub(super) fn install(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    ctx.globals().set(
        "__domSelect",
        Func::new(|html: String, selector: String| -> String {
            let selector = match Selector::parse(&selector) {
                Ok(selector) => selector,
                Err(_) => {
                    return json!({"error":"unsupported or invalid Jsoup CSS selector"}).to_string()
                }
            };
            let document = Html::parse_fragment(&html);
            let nodes: Vec<_> = document
                .select(&selector)
                .map(|node| {
                    let text = node.text().collect::<Vec<_>>().join(" ");
                    json!({"outer":node.html(),"inner":node.inner_html(),
                "text":text.split_whitespace().collect::<Vec<_>>().join(" "),
                "attrs":node.value().attrs().collect::<std::collections::HashMap<_,_>>()})
                })
                .collect();
            json!({"nodes":nodes}).to_string()
        }),
    )?;
    ctx.globals().set(
        "__domResolve",
        Func::new(|base: String, href: String| -> String {
            url::Url::parse(&base)
                .and_then(|url| url.join(&href))
                .map(|url| url.to_string())
                .unwrap_or_default()
        }),
    )?;
    ctx.eval::<(), _>(include_str!("js_dom.js"))
}
