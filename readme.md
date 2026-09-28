## TLegado
终端版阅读3.0，基于 rust 与 ratatui，提供优美的TUI，完善舒适的键鼠交互。

## 致谢
本项目的rust legado3.0实现参考了：https://github.com/hadc188/Reader
本项目ratatui键鼠交互实现方案参考了：https://github.com/xai-org/grok-build

## Licenses
MIT

## TUI 在线演示（react模拟）

演示地址：https://wep-56.github.io/Tlegado/

前端参考代码位于 `TUI-example/Tlegado-TUI`，使用 React + Vite，内容为本地模拟数据，尚未接入 Legado。

本地运行和构建（Node.js 24）：

```powershell
cd TUI-example/Tlegado-TUI
npm ci
npm run dev
# 生成 dist/index.html；本地查看构建产物可执行 npm run preview
npm run build
```

`.github/workflows/pages.yml` 在 `master` 分支的前端目录或工作流发生变更时自动构建并部署，也可以在 GitHub Actions 中手动运行。部署仅上传前端 `dist` 目录，使用单文件构建和相对资源路径，兼容 Pages 的 `/Tlegado/` 子路径。仓库 Settings → Pages 的构建来源设为 **GitHub Actions**。Rust 构建目录、Node 依赖和前端构建产物不纳入版本控制；两个依赖锁文件继续保留。

## 阅读预览

在主项目根目录执行 `cargo run`。书架、发现、搜索结果中按 `Enter` 进入阅读，首页主体中按 `c` 继续阅读。

阅读时左栏替换为章节目录：高亮行表示选中章节，`●` 表示当前正文对应章节。

- `Tab`：目录与正文切换焦点；目录中 `j/k` 或上下键选择，`Enter` 打开。
- 正文 `j/k`：逐行滚动；`Space` / `PageDown` 下一页，`PageUp` 上一页。到章末继续向后翻页或滚动会进入下一章；在章首向前则回到上一章末页。全书首尾不会循环跳转。
- `[` / `]`：上一章 / 下一章；`g/G`：目录或正文首尾。
- `t`：定位并聚焦当前章节；`Ctrl+B`：显示或隐藏目录。
- `q` / `Esc`：返回进入阅读前的页面，并恢复原先的侧栏显隐状态。
- `?`：阅读快捷键帮助。

目前章节标题和正文均为演示内容，不是书籍原文；书架书籍的章节进度仅保留在本次运行中，尚未接入真实书源和持久化存储。
