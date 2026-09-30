# Legado 动态书源能力审计

审计输入是仓库内的 booksources.json 和 booksourcesmost.json，只做离线字段/API 词法盘点，不执行书源脚本、不访问书源网址。原始文件 SHA-256 见 source-capabilities.md。

## 覆盖情况

- 961 条去重后的 JSON 书源版本，926 个非空书源 URL。
- 952 条含 @js:、<js> 或模板动态标记。
- 501 条启用 enabledCookieJar。
- 174 条在规则中出现 java.ajax/java.get/java.post/java.put 等请求或变量 API；其中 145 条明确出现网络 API（不含 get/put 的歧义计数）。
- 183 条配置 loginUrl，其中 136 条是 HTTP 登录页，16 条是 JS 登录入口；7 条配置 loginCheckJs。
- 95 条出现 WebView 相关字段或 API。

这些数字是静态信号，不代表每条书源都能运行，也不等于唯一站点数量；完整数据见 source-capabilities.json。

## 实现前复现的缺口

以下是修改前的基线，不代表当前代码状态。2026-09-30 已落地会话上下文、CookieStore、JS HTTP 响应适配和登录校验第一阶段，详见 ../source-runtime.md；runtime-observations.json 保留为修改前快照。

source_runtime_audit 使用本地回环 HTTP 服务验证了输入到输出的行为，结果在 runtime-observations.json。

1. **JS HTTP 与普通抓取的 Cookie 会话断开。** 普通 reqwest::Client 收到 Set-Cookie 后能得到 cookie-present；JS java.ajax 得到 cookie-absent。当前 JS 使用全局 JS_HTTP_CLIENT，普通书源使用按源隔离的 client。
2. **cookie 对象是空壳。** cookie.getCookie() 始终空串，removeCookie() 不会影响 JS HTTP 会话。参考实现按域名持久化、合并 session cookie，并把 WebView Cookie 同步回 CookieStore。
3. **java.get/java.put 语义不兼容。** Legado 的 AnalyzeRule/AnalyzeUrl 把它们用于书源/书籍/章节变量读写；当前绑定把 java.get(url) 和 java.put(url, body) 当成 HTTP 请求。回环探针 java.put(...);java.get(...) 返回空串。
4. **响应类型不兼容。** Legado 的 java.ajax 返回正文字符串；java.connect 返回 StrResponse，提供 .body()、.code()、.headers()、.url() 和 .isSuccessful()；带 headers 参数的 java.get/post 返回 Jsoup Connection.Response，常见方法是 .body() 和 .statusCode()。修改前 get/post 绑定只返回裸字符串，connect 缺失。不能把 ajax 改成响应对象。
5. **URL 选项只支持对象形式的 headers。** Legado 允许解析字符串/对象选项；对象 headers 当前可用，但 headers JSON 字符串在 JS java.ajax 路径中未被解析。单数 header 是负例，不属于参考 UrlOption 字段。
6. **WebView 请求被静默降级。** RequestSpec 能解析 webView、webJs 和延迟字段，但 fetch_with_client 仍执行普通 HTTP，回环服务收到的结果是 fixture-ok，没有浏览器 JavaScript 执行或明确 unsupported 状态。
7. **登录 API 仍不完整。** 当前 login_book_source 能发起 loginUrl 并运行 loginCheckJs，但 JS 环境缺少 source.getLoginInfoMap、source.putLoginHeader、source.getVariable，也没有 java.startBrowserAwait、java.webView、java.connect。loginUi 的表单/按钮动作无法复现。
8. **共享 JS 库机制已有基础但语义不同。** 当前支持内联脚本和 HTTP jsLib 下载缓存；参考实现按库建立可复用 Rhino scope，并阻止隐式全局变量。当前每次 eval_js 新建 QuickJS Runtime，库只是拼接执行，状态不会跨解析调用保留。

## 建议实现顺序

1. **先统一会话上下文。** 抽出 SourceRuntime（source URL、user namespace、reqwest client、CookieStore、变量/cache、可选 WebView），让普通抓取、JS HTTP、登录和 WebView 共享同一上下文；禁止全局 JS_HTTP_CLIENT 继续承载书源 Cookie。
2. **完成响应适配。** 保留 java.ajax 的字符串返回值；为 connect 提供 StrResponse 方法，为显式 HTTP get/post 提供常用 Jsoup 响应方法，保留 headers、最终 URL、状态码和成功标记。
3. **分离变量 API 与 HTTP API。** java.get/put 按参数/调用来源兼容变量语义，HTTP 请求使用 java.ajax/connect/显式 get 等；补齐 source/book/chapter 的变量链和 source.getKey。
4. **接入 CookieStore。** 实现按源/域名的 get、set、replace、remove，与 reqwest cookie jar 双向同步；enabledCookieJar 决定是否合并 Cookie。
5. **登录 UI 和浏览器能力。** 先支持 loginUi 的字段、按钮 action、login() 和 loginCheckJs；再把 WebView2/Tauri 能力接入 RequestSpec.web_view。在 WebView 尚未支持时应返回结构化错误，不能伪装为普通抓取。
6. **最后做兼容性回归。** 从报告中的高频 API（java.ajax、java.getString、source.getKey、cookie.removeCookie、source.getVariable、java.startBrowserAwait）各抽取本地固定样例，建立单测后再跑真实书源。

## 修改前基线验证

已通过：

cargo test -p reader-rust --locked --test js_compat --test js_http_async --test book_source_headers --test book_source_compat -j 4

结果为 20 个相关测试全部通过。js_http_async 首次失败是 Windows 测试夹具继承监听 socket 非阻塞状态，已在 accepted stream 上显式恢复阻塞模式；修复后通过。

新增诊断命令：

node scripts/audit-source-capabilities.mjs
cargo run -p reader-rust --locked --example source_runtime_audit -- docs/audits/runtime-observations.after.json
