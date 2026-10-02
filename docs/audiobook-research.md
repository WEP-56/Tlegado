# 听书能力调研

本文记录 Tlegado 在终端环境中增加听书能力的技术调研结果。当前仓库是 Rust 2021 + ratatui 0.29 + crossterm 0.28 + Tokio；ratatui 只绘制终端 UI，不提供音频输出、TTS 或媒体会话能力。

## 结论摘要

建议把听书设计成独立于 ratatui 的 `audio` 子系统：主线程只发送控制命令、读取播放事件并刷新 UI；音频输出、系统 TTS、网络 TTS 和缓存都在专用 worker 中运行。

推荐的首个可用版本：

1. 复用上游 Legado 的段落、跳过标点段、章节边界和进度语义。
2. Windows 优先使用 `tts` crate 的 SAPI 后端，macOS 使用 NSSpeechSynthesizer，Linux 使用 Speech Dispatcher；在系统后端不可用时给出明确错误，并保留命令行后端作为可选兜底。
3. BYOK HTTP TTS 使用现有 `reqwest` 请求音频，使用 `rodio` 输出；先完整下载当前段落再播放，稳定后再增加流式预取。
4. 对 HTTP TTS 做按段落缓存，缓存键至少包含 provider、endpoint、model、voice、rate 和文本，避免切换音色后误用旧音频。

## 1. ratatui 与音频播放

### ratatui 本身的边界

ratatui 是终端绘图库，负责 `Frame`、Widget 和布局，不包含音频设备、解码器、播放队列或暂停/恢复接口。音频不能在 `ui::draw` 或 TUI 绘制回调里实现。

当前主循环位于 [`src/main.rs`](../src/main.rs)：每 100 ms 轮询后台事件、处理键盘、调用 `app.on_tick()` 并绘制。音频事件可以沿用这个节奏进入 UI，但播放线程不能依赖绘制帧的生命周期。

### 推荐播放栈

| 层 | 建议 | 用途 |
| --- | --- | --- |
| 输出与队列 | `rodio` | 跨 Windows/macOS/Linux 打开默认输出设备，播放 WAV/MP3/常见压缩音频，提供 `Sink` 的暂停、停止、音量和队列能力 |
| 解码 | rodio 自带解码能力，必要时显式启用 `symphonia` 格式 | BYOK 返回的 MP3/WAV/OGG/FLAC 等音频 |
| 更底层替代 | `cpal` | 只有在需要直接写 PCM、精确混音或自定义音频回调时才使用；实现成本明显高于 rodio |

`rodio` 版本应锁定后再写代码。0.21 系列使用 `OutputStreamBuilder`/mixer 风格 API，旧版本使用 `OutputStream::try_default`；不要同时依赖两套 API。音频 worker 应持有 output stream 和 sink，不能让 stream 在函数返回时析构，否则播放会立即停止。

建议的 worker 命令：`PlayBytes`、`PlayFile`、`Pause`、`Resume`、`Stop`、`SetVolume`、`Shutdown`。worker 事件：`Started(segment_id)`、`Progress(segment_id, offset)`、`Finished(segment_id)`、`Paused`、`Error(message)`。

### 播放线程边界

- `rodio::Sink` 和音频设备对象只在 worker 内访问。
- UI 线程只通过 channel 发命令；不要把 `Sink`、解码器或平台 TTS 对象放进 `App`。
- 网络下载可以由 Tokio 任务完成，但解码和播放状态要通过同一音频 worker 串行化。
- 播放完成必须产生事件，不能用 `Sink::sleep_until_end` 阻塞 UI 或 Tokio runtime。
- 退出顺序应是：停止新请求、向 audio worker 发 `Stop`/`Shutdown`、再恢复终端并结束 Tokio bridge。

## 2. 系统 TTS 调研

### Rust 方案

首选 [`tts`](https://crates.io/crates/tts) 这类跨平台封装：

| 平台 | 后端 | 前置条件与限制 |
| --- | --- | --- |
| Windows | SAPI | 系统需要安装对应语言和声音；通常不需要额外服务。SAPI/COM 对象应在专用线程初始化和销毁 |
| macOS | NSSpeechSynthesizer | 使用系统已安装的声音；可用声音、速率和语言由系统决定 |
| Linux | Speech Dispatcher | 需要运行 speech-dispatcher，并安装开发库/运行时；不同发行版的声音质量和可用性差异较大 |

封装的典型 API 是创建 `Tts`、设置速率/音量/音高、`speak`、`pause`、`resume`、`stop` 和枚举 voices。实际接入时应按目标平台启用依赖并固定版本，因为不同版本的 feature 名称和系统依赖可能变化。应在三套 CI/构建目标上做一次最小编译验证。

Windows 直接调用 `windows` crate 的 SAPI COM 接口也是可行的，但需要处理 `CoInitializeEx`、COM apartment、`ISpVoice` 回调和 HRESULT，维护成本高于 `tts`。建议只在 `tts` 后端无法满足语音枚举或回调需求时再下沉到 SAPI。

### 命令行兜底

- Windows：PowerShell/.NET `System.Speech.Synthesis.SpeechSynthesizer` 可以生成 WAV 或直接播放，但启动进程慢、暂停/进度回调弱，且 PowerShell 版本和 .NET 组件不一致。
- macOS：`say` 易用，但进程管理和音频焦点控制有限。
- Linux：`spd-say` 或 `espeak-ng` 可用，但要求用户安装命令，声音质量取决于系统包。

命令行后端适合诊断和安装不完整的机器，不应作为默认主路径。若启用，必须使用参数数组而不是拼接 shell 字符串，并对进程退出、取消和超时做处理。

### 上游 Legado 行为应保留

上游参考文件：

- [`BaseReadAloudService.kt`](../legado-example/legado-E-clean/app/src/main/java/io/legado/app/service/BaseReadAloudService.kt)
- [`TTSReadAloudService.kt`](../legado-example/legado-E-clean/app/src/main/java/io/legado/app/service/TTSReadAloudService.kt)
- [`TTS.kt`](../legado-example/legado-E-clean/app/src/main/java/io/legado/app/help/TTS.kt)

关键语义：

- 从当前章节生成朗读段落列表，去掉空段；纯标点段不请求语音但仍参与位置推进。
- 从中途位置开始时，只截取当前段落剩余文本。
- 第一段使用队列替换，后续段落追加；系统 TTS 的 utterance `onStart`/`onDone` 驱动进度和下一段。
- 当前章节结束后自动跳到下一章；全书末尾停止。
- 支持暂停、恢复、上一段、下一段、上一章、下一章、语速调整和定时停止。
- 进度使用正文字符位置，而不是终端行号。Tlegado 的 [`Reader::position`](../src/reader.rs) 已使用 Unicode scalar offset，听书状态应复用同一坐标系。

系统 TTS 通常直接向系统音频设备输出，不需要 rodio；但它的事件模型应转换成统一的 audio worker 事件，这样 UI 不必区分系统 TTS 和网络音频。

## 3. BYOK TTS 模型与播放

### 请求模型

不要把 BYOK 绑定到某一家服务。抽象成 provider 配置：

```text
id / name
endpoint
api_key_env 或外部密钥引用
model
voice
response_format (wav/mp3/opus/pcm)
speed / extra_headers / request_template
```

OpenAI-compatible provider 的基本请求形状通常是：

```json
{
  "model": "tts-1",
  "input": "当前段落文本",
  "voice": "alloy",
  "response_format": "wav",
  "speed": 1.0
}
```

请求地址、认证头和字段名必须可配置，才能覆盖 OpenAI、Azure、阿里云、百度、ElevenLabs 以及自托管 OpenAI-compatible 服务。返回 `Content-Type` 是 `application/json` 或 `text/*` 时，应读取错误正文并停止当前段落，不能把错误 JSON 交给解码器。

### 播放和缓存策略

首版采用“当前段落完整下载 -> 校验 -> rodio 播放”的策略：错误容易定位，也不依赖压缩格式的实时解码。网络任务放在 Tokio，音频 worker 只接收已经下载的临时文件或字节。

后续可增加两级队列：当前段落必须先准备，后台预取后续 1 到 10 段；切章时取消旧 generation，避免旧请求覆盖新章节。流式播放只有在首包延迟成为实际问题后再做，因为 MP3/Opus 分片、取消和解码错误都需要额外状态机。

缓存文件建议放在数据目录的 `tts-cache/`，文件名使用稳定哈希，例如：

```text
sha256(provider_id + endpoint + model + voice + rate + normalized_text + format)
```

不要把 API key 放进哈希、日志或普通配置导出。优先从环境变量读取；若增加 UI 保存密钥，Windows 使用 Credential Manager，macOS 使用 Keychain，Linux 使用 Secret Service/keyring，不能把明文密钥写入 SQLite 或 JSON。

### 上游 HTTP TTS 可复用的设计

参考 [`HttpReadAloudService.kt`](../legado-example/legado-E-clean/app/src/main/java/io/legado/app/service/HttpReadAloudService.kt) 和 [`HttpTTS.kt`](../legado-example/legado-E-clean/app/src/main/java/io/legado/app/data/entities/HttpTTS.kt)：

- URL/请求体可由 `speakText`、`speakSpeed` 等变量生成。
- 可配置响应 `contentType`、请求头、登录脚本和错误检查脚本。
- 非流式模式将每段保存后组成播放队列；流式模式边下载边加入播放器。
- 缓存键包含章节、URL、语速和文本；下一章预取若干段。
- 网络错误有重试上限，连续失败后暂停，空文本使用静音音频占位。

Rust 首版不应直接复制 Android 的 JS 登录脚本系统。先提供结构化 headers/body 模板和 OpenAI-compatible 模式；需要 Legado 兼容 URL 模板时，再限定变量集合并用 JSON/URL 编码器生成请求，避免在 TUI 进程内执行任意脚本。

## 4. 适合本项目的模块边界

当前 [`src/jobs.rs`](../src/jobs.rs) 的 bridge 负责书籍网络读取和持久化，[`src/live.rs`](../src/live.rs) 把后台事件映射到 `App`。建议新增独立 `src/audio.rs`，不要把音频下载和设备对象塞入 `jobs.rs`：

```text
App / live.rs
  -> AudioHandle::send(Command)
Audio worker
  -> Segmenter (章节文本 -> Segment{id, text, start_offset})
  -> SpeechBackend (SystemTts | HttpTts)
  -> Player (System output or rodio)
  -> AudioEvent channel
main loop
  -> poll AudioEvent
  -> update reader/audio state
```

需要在 `Reader` 增加一个面向听书的只读接口，返回当前章节原文、章节字符偏移和下一章是否可用；UI 不应直接访问 `real` 的内部结构。播放状态至少包括 `Stopped`、`Preparing`、`Playing`、`Paused`、`Error`，并带有 chapter、segment、字符偏移和 provider 信息。

建议快捷键先沿用上游含义：`p` 播放/暂停，`[`/`]` 上一段/下一段，`{`/`}` 上一章/下一章，`-`/`=` 调速，`s` 停止。最终键位需检查当前阅读器已占用的按键，避免覆盖现有切章和换源操作。

## 5. 分阶段实现与验证

### P0：本地播放状态

- 段落切分、标点过滤、Unicode offset、章节跳转和取消 generation。
- fake speech backend + fake player 单元测试，验证暂停、跳段、切章和末尾停止。
- TUI 只显示简短状态，不在终端输出后台日志。

### P1：系统 TTS

- Windows SAPI 优先，补 macOS/Linux 构建和运行时错误提示。
- 验证中文声音不存在、初始化失败、暂停恢复和退出时资源释放。
- 验证系统 TTS 回调与 Reader 位置一致。

### P2：BYOK HTTP TTS

- OpenAI-compatible JSON + WAV/MP3 响应。
- reqwest 超时、取消、重试上限、Content-Type/JSON 错误识别。
- rodio 播放、缓存、当前段落失败重试和下一段预取。

### P3：体验增强

- provider 管理、语音/速率选择、缓存清理、环境变量/系统密钥环。
- 流式播放、媒体键/音频焦点（平台可用时）、后台继续朗读。

## 6. 当前实现：系统听书、网络听书与 TUI

当前已实现 `Reader` 中的段落导航与听书状态、独占按键路由、底部播放控制器、段落高亮和视口跟随，并接通系统 TTS、BYOK HTTP TTS 与 rodio 播放，以及播放完成驱动的自动切段、切章。网络后端的配置示例与使用边界见 [BYOK 网络听书](byok-tts.md)。

实际落地时选择 Windows 原生 SAPI，而非额外的跨平台 TTS 封装：通过 `windows` 0.62.2 调用 `ISpVoice` 的异步 `Speak`、`Pause`、`Resume`、`SetRate` 和 `GetStatus`，在专用 MTA 线程创建和释放全部 COM 对象。正文按普通文本传入，禁止当作 XML、文件名解析。为降低非 Windows 构建依赖，macOS/Linux 当前先提供 `say` / `espeak-ng` 子进程兼容后端，无需链接 Speech Dispatcher 开发库；原生平台后端可继续替换这一实现。

`src/audio.rs` 中的 controller 只向 worker 发布最新期望状态。每次切段、切章、停止或错误重试都生成新的任务代次；主线程拒收过期的播放事件。worker 以 20 ms 间隔检查系统完成状态，UI 仍按原事件循环刷新。退出会唤醒、停止并回收 worker，子进程不向终端写日志。暂停后切段不会意外恢复；暂停期间到达的完成事件留待用户恢复后推进。

- 阅读时 `p` 启动；听书时 `p` / 空格暂停或继续，`[` / `]` 切段，`{` / `}` 切章。暂停状态下切段、切章保留暂停。
- `-` / `+` / `=` 设置倍速；`t` 循环关闭、15、30、60、90 分钟。倒计时在暂停时继续，到期恢复文字阅读。
- `s` 停止并保留阅读器；`q` / `Esc` 退出阅读；`Ctrl+C` 保存并退出。其他键与滚轮事件在听书模式下被拦截，自动阅读也会停用。
- 控制器根据终端宽度自动分行；暂停时提示改为“继续”。语音准备中和章节加载中分别显示状态。系统语音错误会暂停并持续显示错误，可用 `p` 重试；章节加载失败保留原段落，可再次切章重试。
- 播放片段起点存为原文章节正文的 Unicode scalar 偏移，保存进度时不以屏幕行号代替。新章节只有加载成功后才切换段落，高亮会随终端宽度和横向/纵向排版重新定位。

### 段落规则与后端边界

真实正文按原文换行分段，跳过仅含空白的段；CRLF 中的 CR 属于空白，不产生额外段。章节标题不计入正文朗读段。终端折行、行距与段间距不创建新的段落；演示正文在排版时记录源段落边界，不再从渲染后的空行推断。

长段落在音频后端按最多 480 个 Unicode 字符拆分，优先选择后半段的句末或空白边界；所有片段共享原段落标识，高亮和上一段/下一段仍按源段落工作。纯标点片段跳过合成，但完成事件仍推进段落。朗读文本来自净化后的章节正文，沿用净化引擎的单调原文位置映射，不发送终端装饰、缩进和章节标题。片段开始时更新原文位置并跟随视口；当前不提供逐字回调，因此保存精度是合成片段起点。

### 系统声音与平台边界

- Windows 默认识别中文内容并选择已安装的中文 SAPI 声音；可用声音取决于系统兼容语音包。没有中文声音时明确报错，不静默跳过中文。
- `TLEGADO_TTS_VOICE` 可指定声音。Windows 使用完整 SAPI 名称，macOS 使用 `say` 名称，Linux 使用 espeak-ng voice ID（默认 `cmn`）。Windows 指定名称不存在时错误中列出可用名称。
- Windows 的倍率映射到 SAPI 的引擎速率，为近似值；macOS/Linux 的速率以每分钟词数映射，下一个片段生效。
- Unix 后端通过参数数组和 stdin 传递文本，不拼接 shell。暂停/继续只向自己的未回收子进程发送 SIGSTOP/SIGCONT；停止会终止并回收子进程。音频设备已缓冲的少量声音可能不立即停止。

测试覆盖 Unicode 与 CRLF、净化文本与原文偏移、长段落拆分、暂停恢复与调速、过期事件隔离、失败重试、末尾停止、异步切章、快捷键屏蔽和多宽度/多主题展示。Windows 已通过实际中文短句的播放、暂停、恢复、调速和停止测试；该测试默认忽略以避免普通测试运行时发声，可用 `cargo test --locked --bin tlegado windows_sapi_smoke -- --ignored --nocapture` 手动运行。macOS/Linux 后端尚未在对应平台完成编译与音频实测。

### BYOK 网络后端

- 通过侧栏“听书设置”配置后端，Ctrl+S 保存并立即生效；也可通过 `--tts-config`、`TLEGADO_TTS_CONFIG` 或数据目录中的 `tts.json` 加载。未配置时仍使用系统语音。支持兼容语音接口的默认 JSON 请求和自定义 JSON 模板；凭据支持环境变量或在 TUI 中遮罩输入、明文保存到本地配置。
- reqwest 在独立异步运行时下载，rodio 在播放 worker 中持有输出设备与播放器。下载期间显示准备状态，完成下载和解码校验后才报告开始播放；暂停期间下载完成不会自动发声。
- 请求限制超时、响应大小与重试次数，不跟随重定向；停止、切段、切章与退出取消不再需要的请求。错误提示不包含原始服务商响应、完整 URL 或密钥。
- 提供按请求内容区分的磁盘缓存和可选的单片段预取。预取默认关闭，匹配结果可跨段复用；服务端倍速从下一个合成片段生效。
- 本地 HTTP 测试覆盖请求、错误、重试、缓存、暂停、取消和预取；Windows 已完成下载、解码、实际设备播放、暂停与恢复测试。真实服务商模型和 macOS/Linux 音频设备尚待实测。Linux 构建因 rodio 新增 ALSA 开发依赖，发布工作流已安装 `libasound2-dev` 与 `pkg-config`。

## 后续建议

当前 TUI 已支持单个后端配置与声音名称编辑。后续优先验证真实服务商模型与 macOS/Linux 平台，再按需要增加多服务商配置管理、声音列表和流式播放。ratatui 承担状态展示、配置编辑和按键路由；系统语音与网络音频共用 worker 状态与事件抽象，沿用已确认的段落控制与高亮交互。
