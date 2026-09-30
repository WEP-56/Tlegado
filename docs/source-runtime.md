# 动态书源运行时：会话基础设施与持久化

实现日期：2026-09-30。第一阶段统一普通抓取、JavaScript HTTP 和登录校验的会话基础设施；第二阶段接入会话持久化。不代表所有 Legado 动态书源已经兼容。

## 已实现

- BookService 按用户 namespace 和书源标识保存 SourceSession。普通 HTTP、动态 header、JS HTTP、解析规则和登录校验共享这一会话，源相同但用户不同、用户相同但源不同都隔离。
- SourceRuntime 使用 Tokio task-local 跨 await 传递；同步 JS 执行期间使用可恢复的 thread-local。一次操作共享规则临时变量，新操作不会继承旧的 java.get/put 临时数据。
- CookieStore 处理 domain、path、Secure、过期和 Set-Cookie；cookie.getCookie、setCookie、replaceCookie、removeCookie 与普通请求共享数据。setCookie 替换当前域，replaceCookie 合并同名项。手工 Cookie 导入先完整验证，失败不破坏旧值。
- enabledCookieJar=true 才自动保存响应 Cookie。显式导入的 Cookie 在关闭自动保存时仍可发送；不再把旧 Cookie 固定注入每个请求头。
- java.ajax 返回正文字符串；java.connect 提供 body()/code()/url()/headers()/isSuccessful()。java.get(url, headers)、post、head 提供响应对象及 statusCode()/header()。ajax/connect 跟随重定向，显式 get/post/head 不自动跟随。
- JS HTTP 复用 URL 选项解析、书源 headers、代理配置、超时、重试和字符集解码。headers 支持对象或 JSON 字符串。动态 header 内请求不会递归重新执行同一 header 规则。
- java.get(key)/put(key,value) 用于操作内变量；source.getKey() 返回书源标识，book.bookUrl 单独保存当前书籍地址。独立 RuleEngine 调用也建立书源上下文。
- source.getVariable/setVariable、source.get/put、cache.get/put 和 JS 库文本缓存保存在源会话中，避免全局缓存跨用户共用。
- source.putLoginHeader/getLoginHeader/getLoginHeaderMap/removeLoginHeader 接入实际请求。登录 headers 覆盖源 headers，请求选项或显式请求参数再覆盖登录 headers。登录 Cookie 放入共享 jar，删除后不会被静态 header 重新注入。
- loginCheckJs 收到可调用响应方法的 result；异常或 false 返回错误，登录接口也不再把非成功 HTTP 状态报告为成功。保留原有返回正文字符串及序列化响应的兼容路径。
- RequestSpec 要求 WebView/webJs 时明确返回 unsupported capability，避免普通 HTTP 冒充浏览器执行。

## 第二阶段：会话持久化

- BookService 创建的会话按需加载到 storage_dir/data/{user_namespace}/source_sessions/{SHA256(normalized_source_url)}.json。文件名不包含原始书源 URL 或查询参数；快照记录版本号及用户/书源联合身份摘要，拒绝版本或身份不匹配的数据。namespace 必须是单个安全路径组件。
- Cookie、登录 headers、source.getVariable/setVariable、source.get/put、cache.get/put 和 java.ensureGlobalVariable 的缓存同步保存；普通 HTTP 与 JS HTTP 的响应 Cookie 使用相同事务。session cookie 也保留到下次启动，过期 Cookie 加载时丢弃，并保留域、路径和 Secure 等原属性。
- 登录 header 中的 Cookie 只保存空值标记，实际值由 CookieStore 维护。删除 Cookie 后不会留下可重新注入的旧 header；清除认证会同时删除 Cookie 和登录 headers，并保留源变量及 cache。
- 每次修改在同一会话锁内复制候选状态，写入同目录唯一临时文件、同步文件内容后原子替换；磁盘提交成功后才更新内存。失败保留旧文件和旧内存，清理本次临时文件。Windows 下目标文件被占用导致替换失败也有回归覆盖。
- 无法读取、JSON 损坏、未知版本或身份不匹配时保留原文件并阻止该会话继续写入；请求返回会话错误。修复存档后需重建服务/重启应用再加载。响应 Cookie 回调不能直接返回错误，因此由请求边界报告保存失败，错误内容不带 Cookie 或磁盘路径。
- java.get/put 的操作临时变量和已下载 jsLib 文本不落盘；重启后重新建立。独立 SourceSession::default 仍是内存会话，供无存储目录的独立解析及测试使用。
- 快照是未加密 JSON，可能含登录凭据；Unix 新建目录/文件权限分别为 0700/0600，Windows 使用所在目录的 ACL。当前使用同步磁盘写入，只串行化同一 SourceSession 的修改，尚无跨进程文件锁；不支持多个进程或独立 BookService 同时修改同一用户/书源存档。

## 验证方式

crates/legado-core/tests/source_runtime.rs 使用独立线程上的本地 HTTP 夹具，覆盖 Cookie 双向共享、用户/源隔离、禁用自动 Cookie、响应方法、重定向、POST、字符集、header 优先级、变量寿命、task-local 切换与取消、源/书籍身份、登录失败和 WebView 拒绝。

回归命令：cargo test --workspace --locked --offline -j 4

第一阶段验证结果：219 项通过、0 项失败、1 项真实站点联网测试跳过。其中核心库及集成测试 145 项、TUI 测试 74 项；运行时回归 12 项。原始输出保留在 [source-runtime-tests.log](audits/source-runtime-tests.log)。

第二阶段增加 8 项运行时回归，覆盖服务重建恢复、用户/源隔离、删除后重启、Cookie 属性及过期、损坏/身份/版本校验、并发修改、写盘失败回滚和 Windows 替换失败。完整工作区回归结果为 227 项通过、0 项失败、1 项真实站点联网测试跳过（核心库及集成测试 153 项、TUI 74 项）。输出见 [session-persistence-tests.log](audits/session-persistence-tests.log)；修改文件的 rustfmt、JS 语法检查及 git diff --check 均通过。

审计基线保存在 [runtime-observations.json](audits/runtime-observations.json)，当前运行结果写入 [runtime-observations.after.json](audits/runtime-observations.after.json)，不覆盖原始快照。两次探针中普通 HTTP 使用的上下文不同：基线模拟旧的独立 client，当前显式使用 SourceRuntime；BookService 真实调用路径另由上述集成测试验证。

js_http_async 另在全新子进程中验证 current-thread 和 multi-thread Tokio 的 JS 请求，以及终端无额外输出。真实站点测试保留 ignore，离线回归不访问外部书源。

## 后续阶段

- 会话文件跨进程并发锁、异步写入及凭据加密尚未接入；当前持久化行为与约束见第二阶段说明。
- 登录表单 loginUi、getLoginInfoMap、JS loginUrl 动作执行，以及浏览器交互/WebView 后端尚未实现。
- java.getString 等完整规则 API，以及 book/chapter/source 变量优先级链尚未补齐；现有 java.get/put 只覆盖操作内临时变量。
- QuickJS 仍按次建立上下文，jsLib 缓存的是脚本文本，不是参考实现的复用 Rhino scope。
- JS HTTP 的阻塞客户端安全地在工作线程建立和释放；同步 JS 在单线程 Tokio 上仍会等待网络完成。整段规则求值的异步调度、取消和 JS/native 共用限速器是后续任务。
- get/post 响应仅覆盖常用方法，没有完整复刻 Jsoup 的 DOM、cookies、byteData 等 API。发现适配另增加了独立的常用只读 Jsoup.parse/select 接口，范围见 [discovery.md](discovery.md)。为兼容旧脚本，body/code 等值使用可调用包装；旧式属性访问的严格等号和布尔真值不能等同 Java/Rhino 的全部语义。
- 此阶段验证本地行为与既有业务回归，不承诺静态盘点中的 926 个源全部可用。
