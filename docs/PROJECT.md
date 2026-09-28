# Tlegado：Rust + Ratatui 终端版 Legado

## 1. 项目定位

Tlegado 是一个只面向终端的 Legado 阅读器。它复用 Legado/Reader 已验证的书源协议、解析规则和阅读数据模型，以 Rust 提供完整业务能力，以 Ratatui + Crossterm 提供跨平台 TUI。

首要目标是让用户在 SSH、服务器终端和本地终端中完成完整阅读闭环：导入或配置书源、搜索/发现书籍、加入书架、获取目录和正文、保存阅读进度，并能管理书源、净化规则和阅读偏好。

非目标：首版不提供 WebView、桌面窗口、图片化封面墙或独立 HTTP 服务；不复制 Tauri/Vue 的 IPC 层。TUI 直接调用 Rust service，所有网络和文件操作通过异步任务执行。

## 2. 参考项目与可复用结论

### 2.1 `legado-example/reader`

这是领域能力的主要参考实现，当前代码已经包含：

- `model/`：Book、BookChapter、BookSource、BookGroup、SearchBook、规则对象等 Legado 数据模型。
- `parser/`：CSS、XPath、JSONPath、Regex、JavaScript 的规则识别和执行；`RuleEngine` 负责搜索、发现、书籍详情、目录和正文解析。
- `crawler/`：HTTP 客户端、请求构造、URL 分析、Cookie/响应处理和反爬提示。
- `service/`：书籍、书源、书架分组、本地 TXT/EPUB/PDF、阅读统计、用户和更新业务。
- `storage/`：SQLite 迁移、文件缓存、书籍/章节/备份等文件存储。
- `util/` 与 `error/`：文本清理、URL/哈希、统一错误模型。

复用原则：优先复用模型、解析器、crawler、service 和 storage；TUI 只新增终端启动、状态适配和渲染层。书源 JSON 的兼容迁移、规则前缀（`@css:`、`@xpath:`、`@json:`、`@regex:`、`js:`）和请求行为属于核心兼容性，不在 TUI 层重新实现。

### 2.2 `TUI-example/grok-build`

重点参考其 Ratatui/Crossterm 应用的组织方式，特别是：

- 事件循环集中读取 `KeyEvent`、`MouseEvent`、Resize 和 Tick，再交给当前页面/弹层处理。
- 页面状态与绘制分离；组件在绘制时登记可点击/可滚动区域，鼠标处理根据矩形命中区域派发动作。
- 设置/列表交互保持键盘和鼠标路径一致，并为鼠标路径编写状态级测试。
- 长任务通过后台任务、进度/结果消息回传 UI，避免阻塞绘制线程。

Tlegado 不复制其业务模块，而采用这些交互和事件分发约定。

### 2.3 `TUI-example/Tlegado-TUI`

这是页面信息架构和视觉范例。当前范例明确了：

- 全局侧栏：首页、全部/本地/网络书籍、搜索、各书源发现、阅读历史、书源管理、净化规则、阅读偏好。
- 全局焦点：`Tab` 在侧栏和主体之间切换；`Ctrl+B` 显示/隐藏侧栏；`/` 搜索；`?` 帮助；`Esc` 返回上一级。
- 列表约定：`j/k` 或方向键移动，`Enter` 打开，`h/l` 切换标签或筛选，`g/G` 跳到首尾。
- 阅读器约定：空格/`j` 下一页，`k` 上一页，`[`/`]` 切章节，`t` 目录，`f` 沉浸模式，`a` 加入书架，`q` 退出。
- 视觉约定：窄侧栏 + 主面板、带标题的边框、底部快捷键提示、列表选中态、异步操作状态和 Toast/状态栏反馈。

React 范例只作为交互规格和页面草图，不作为运行时依赖；所有页面最终由 Ratatui 渲染。

## 3. 总体架构

```text
main / terminal bootstrap
        |
AppRuntime (event loop, terminal, async task bridge)
        |
AppState (route, focus, modal, notifications, loading jobs)
        |
Page + Component handlers        Ratatui renderers
        |                           |
Command/Action -------------------+
        |
Domain services (book/source/search/reader/settings)
        |
reader-rust domain crates: model, parser, crawler, storage, service
```

建议的 TUI crate 目录：

```text
src/bin/tlegado.rs          # 终端入口
src/tui/
  app.rs                    # AppState、路由、焦点、弹层
  event.rs                  # Crossterm 事件和 Tick
  action.rs                 # 领域无关的 UI Action
  runtime.rs                # 终端初始化、恢复和异步消息桥
  keymap.rs                 # 全局/页面快捷键
  layout.rs                 # 页面布局和可命中区域
  render/                   # Ratatui 绘制
  pages/                    # home、shelf、search、discover、reader 等
  widgets/                  # sidebar、list、tabs、dialog、status、help
  jobs.rs                   # 搜索、更新、校验、章节加载任务
```

首版可以在当前 crate 内落地；当 TUI 与领域代码稳定后，再将领域 crate 抽为共享库，避免为终端重复维护书源解析逻辑。

## 4. 核心状态模型

`AppState` 至少包含：

- `route`：当前页面和页面参数。
- `focus`：`Sidebar`、`Main`、`Modal`、`Reader`。
- `sidebar_visible`、`zen_mode`、`help_visible`。
- 当前选中行、列表滚动偏移、当前标签/筛选。
- `reader`：书籍、章节、页码/滚动偏移、目录弹层。
- `jobs`：任务 ID、阶段、进度、错误和结果；UI 不直接持有网络 future。
- `notice`：成功、错误、信息和持续时间。
- 从 service 加载的书架、书源、历史、净化规则和偏好缓存。

页面应通过 `Action` 修改状态，例如 `Navigate`、`MoveSelection`、`OpenBook`、`ToggleSource`、`StartSearch`、`LoadChapter`、`ShowModal`、`Quit`。页面绘制函数只读状态并登记区域，不直接执行网络请求。

## 5. 事件与鼠标契约

事件优先级固定为：退出/全局快捷键 -> 模态框 -> 当前页面 -> 未处理事件。输入来源包括键盘、鼠标、终端尺寸变化和定时 Tick。

每个可交互组件必须同时定义：

1. 键盘动作到 `Action` 的映射。
2. 绘制后的 `Rect`/命中区域。
3. 鼠标点击、滚轮、拖动（若支持）到同一 `Action` 的映射。
4. 区域越界、空列表、不可用项和弹层关闭行为。

鼠标规则：左键选择/打开，双击只用于明确的打开动作；滚轮滚动当前列表；点击遮罩关闭非危险弹层；右键是否启用需逐页面决定，不能让右键行为与键盘语义冲突。终端不支持鼠标时，所有功能仍必须可用。

## 6. 页面范围

### MVP 页面

- 首页：最近阅读、书架入口、搜索/发现入口、可用书源和更新状态。
- 书架：全部/本地/网络筛选，分组、排序、选中书籍、打开阅读器。
- 搜索：关键词输入、并发书源搜索、结果列表、加入书架、打开详情/阅读。
- 发现：按书源和分类浏览。
- 阅读器：章节正文、翻页/滚动、目录、章节切换、进度保存、沉浸模式。
- 历史：最近阅读记录和恢复到章节。
- 书源管理：启用/禁用、探索开关、测试单个/全部书源、导入/导出 JSON。
- 阅读偏好：主题、字号/宽度、行距、分页模式、简繁转换、净化和自动翻页。

### 后续页面

净化规则编辑、书源调试 trace、WebDAV 同步、备份恢复、统计图表、RSS 和本地文件导入。

## 7. 数据与任务边界

- SQLite 继续作为结构化数据存储；文件缓存保存正文、封面和本地书籍元数据。
- service 层负责事务、缓存、限速、Cookie、解析和错误转换；TUI 不访问 SQL、reqwest 或规则引擎内部对象。
- 搜索、书源校验、更新和章节加载都以可取消后台任务运行；完成后通过消息提交结果，过期任务不得覆盖新页面状态。
- 本地 TXT/EPUB/PDF 继续使用现有 service；终端只提供路径选择/命令行参数和结果反馈。
- 所有用户可见错误都转换为可读的状态栏消息；详细请求/解析 trace 进入调试页面或日志。

## 8. 里程碑

1. **骨架**：Cargo 二进制、终端初始化/恢复、事件循环、全局 keymap、空页面和退出流程。
2. **可读闭环**：复用现有 storage/service，实现书架、搜索、章节加载、阅读器和进度保存。
3. **来源闭环**：书源导入/导出、启停、发现、校验、错误和加载状态。
4. **交互完善**：鼠标命中区域、滚轮、弹层、帮助、Toast、窄终端布局和无鼠标兼容。
5. **质量与发布**：单元测试、状态级事件测试、解析回归测试、终端快照/人工验收、Windows/macOS/Linux 打包。

每个里程碑都要求 `cargo fmt --check`、`cargo clippy`、相关测试和手动终端验收通过后再进入下一阶段。

## 9. 测试策略

- 领域回归：直接复用现有 parser、book-source compatibility 和本地书籍测试。
- 状态测试：给定初始 `AppState` 和事件序列，断言路由、焦点、选中项、弹层和任务状态。
- 鼠标测试：为每个交互组件构造 `Rect`，验证点击/滚轮与对应键盘动作产生相同 `Action`。
- 任务测试：模拟成功、超时、取消、过期结果和部分书源失败。
- 终端验收：80x24、120x40、窄宽度；中文/等宽字体；无鼠标；颜色不足终端；网络失败。

## 10. 约束与风险

- 终端宽度、中文字符宽度和字体差异会影响布局；所有固定区域必须使用 Ratatui `Rect` 计算，禁止依赖字符串长度猜坐标。
- 书源 JavaScript 执行和不可信网页内容必须沿用现有隔离/错误处理边界；TUI 不扩大脚本权限。
- 网络搜索可能返回大量结果；列表必须支持截断、分页/虚拟化或增量消息，不能在渲染线程等待全部结果。
- 终端能力检测（鼠标、真彩、Unicode）应降级到可读文本，不影响业务操作。
- 现有参考项目文件存在编码显示不一致问题；新代码和文档统一 UTF-8，命令行输出避免依赖系统代码页。

## 11. 开发约定

- 领域命名优先与 `reader-example` 保持一致，TUI 名称只描述展示和交互。
- 新增页面先写状态转移和快捷键，再写绘制；新增鼠标操作必须补同语义键盘操作。
- 不在组件中启动不可追踪的后台任务；任务统一登记、可取消并回传结果。
- 提交前检查终端退出恢复、错误路径和空状态；不要只验证有数据、宽终端和成功网络。

