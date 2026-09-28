import { Box } from "./tui";

const GROUPS: [string, [string, string][]][] = [
  ["全局", [["tab", "切换 侧栏 / 主体 焦点"], ["/", "打开书籍搜索"], ["?", "显示 / 关闭帮助"], ["esc", "返回侧栏"], ["ctrl+b", "显示 / 隐藏侧栏"]]],
  ["列表", [["j k ↑ ↓", "上下移动"], ["h l ← →", "切换分组 / 分类 / 选项"], ["g G", "跳到首 / 尾"], ["enter", "打开 / 确认"], ["space", "启用 / 禁用"]]],
  ["阅读", [["space j →", "下一页"], ["k b ←", "上一页"], ["[ ]", "上 / 下一章"], ["t", "目录"], ["f", "沉浸模式"], ["q", "退出阅读"]]],
  ["搜索", [["/", "跳转并聚焦搜索框"], ["enter", "提交关键词 / 打开试读"], ["j k", "浏览搜索结果"], ["a", "加入选中书籍到书架"]]],
];

export function Help({ onClose }: { onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <Box
        active
        className="fade-in max-h-[90vh] w-[min(760px,100%)] overflow-y-auto bg-bg px-6 pt-5 pb-5"
        title={<span className="font-semibold text-hi">快捷键</span>}
        footer="esc / ? 关闭"
      >
        <div className="grid gap-x-10 gap-y-5 sm:grid-cols-2" onClick={(e) => e.stopPropagation()}>
          {GROUPS.map(([g, items]) => (
            <div key={g}>
              <div className="mb-1 text-[12px] text-dim">── {g}</div>
              {items.map(([k, d]) => (
                <div key={k} className="flex justify-between gap-4">
                  <span className="text-fg">{d}</span>
                  <span className="shrink-0 text-accent">{k}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </Box>
    </div>
  );
}
