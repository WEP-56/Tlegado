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

 `cargo run`

## 致谢
本项目的rust legado3.0实现参考了：[Reader](https://github.com/hadc188/Reader)

## Licenses
MIT