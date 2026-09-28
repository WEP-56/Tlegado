## TLegado
终端版阅读3.0，基于 rust 与 ratatui，提供优美的TUI，完善舒适的交互。


## TUI 前端演示

演示地址：https://wep-56.github.io/Tlegado/

此站点仅为react静态模拟，提供使用前预览，不代表真实TUI视觉、交互效果，真实效果以安装后为准。
![截图](TUI-example/Tlegado-TUI/首页.png)

## 键盘操作介绍

书架、发现、搜索结果中按 `Enter` 进入阅读，首页主体中按 `c` 继续阅读。

阅读时左栏替换为章节目录：高亮行表示选中章节，`●` 表示当前正文对应章节。

- `Tab`：目录与正文切换焦点；目录中 `j/k` 或上下键选择，`Enter` 打开。
- 正文 `j/k`：逐行滚动；`Space` / `PageDown` 下一页，`PageUp` 上一页。到章末继续向后翻页或滚动会进入下一章；在章首向前则回到上一章末页。全书首尾不会循环跳转。
- `[` / `]`：上一章 / 下一章；`g/G`：目录或正文首尾。
- `t`：定位并聚焦当前章节；`Ctrl+B`：显示或隐藏目录。
- `q` / `Esc`：返回进入阅读前的页面，并恢复原先的侧栏显隐状态。
- `?`：阅读快捷键帮助。

## 本地开发

默认启动真实模式，首次运行没有内置书源。先准备 Legado JSON 书源文件：

```powershell
cargo run -- --import-source "C:\Books\书源.json"
```

也可以直接运行 `cargo run`，进入书源管理后按 `Tab` 将焦点移到主体，按 `i` 输入 JSON 文件路径。支持单个对象、数组及旧版书源迁移；同 URL 书源覆盖更新。每个文件先整体校验，再以事务导入，文件上限 16 MiB。

- `/` 输入关键词，`Enter` 提交多源搜索，再按 `Enter` 打开选中书籍。
- 发现侧栏使用书源提供的分类和规则，切换分类后自动加载。
- 打开正文后自动加入书架；进度约每 2 秒保存，切章、离开阅读和退出时也保存。历史由保存的阅读进度生成。
- 阅读正文和目录会缓存；已缓存章节可断网读取，重启后恢复章节和字符位置。
- 书源管理中 `i` 导入、`o` 导出到新文件、空格启停、`e` 切换探索、`v` 查看 JSON。导出不会覆盖已有文件。
- `Esc` 取消搜索/打开操作；阅读中返回上个页面。取消后丢弃旧结果并停止调度，已发出的请求正常结束。`Ctrl+C` 保存进度并退出。

数据默认保存在用户目录的 `.tlegado`，可用 `TLEGADO_DATA_DIR` 或 `--data-dir` 指定。命令行导入支持重复传入多个 `--import-source`：

```powershell
cargo run -- --data-dir "E:\Reader Data" --import-source "C:\Books\书源.json" --import-only
cargo run -- --data-dir "E:\Reader Data"
cargo run -- --demo
cargo test --workspace --locked --lib --bins --tests -j 4
```

`--demo` 保留原离线演示，不读写真实数据。正式构建使用纳入版本控制的 `crates/legado-core`，不依赖本机的 `legado-example/` 参考目录。

当前是首个真实阅读闭环：搜索/发现只加载第一页，每源最多 200 条、界面最多 2000 条，并发上限 4。来源切换、后续分页、本地 TXT/EPUB/PDF 的 TUI 入口、分组管理、批量校验/更新、净化规则与偏好持久化仍待接入。鼠标交互适配继续延后。自动验收使用本地 HTTP 测试书源，不代表所有第三方书源兼容性或跨平台人工终端验收已通过。

## 致谢
本项目的rust legado3.0实现参考了：[Reader](https://github.com/hadc188/Reader)

## Licenses
MIT
