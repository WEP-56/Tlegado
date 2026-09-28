import { useEffect, useState } from "react";
import { useApp } from "@/store";
import { Box, Empty, Hints, Row, useKeys, useScrollIntoView } from "@/components/tui";

export function History({ focused }: { focused: boolean }) {
  const { history, setHistory, books, openReader, toast } = useApp();
  const [sel, setSel] = useState(0);
  const [confirm, setConfirm] = useState<null | "one" | "all">(null);
  const ref = useScrollIntoView(sel);
  const cur = history[sel];

  useEffect(() => { if (sel >= history.length) setSel(Math.max(0, history.length - 1)); }, [history.length, sel]);

  const open = () => {
    if (!cur) return;
    const b = books.find((x) => x.id === cur.bookId);
    if (!b) return toast(`《${cur.title}》已不在书架中`, "err");
    openReader(b, cur.chapterIndex);
  };

  useKeys(focused, (e) => {
    const k = e.key;
    if (confirm) {
      if (k === "y") {
        if (confirm === "all") { setHistory([]); toast("已清空阅读历史", "err"); }
        else if (cur) { setHistory((h) => h.filter((x) => x.id !== cur.id)); toast("已删除 1 条记录"); }
      }
      setConfirm(null);
      e.preventDefault();
      return;
    }
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(history.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "Enter") open();
    else if (k === "d" && cur) setConfirm("one");
    else if (k === "D" && history.length) setConfirm("all");
    else return;
    e.preventDefault();
  });

  let lastDay = "";
  return (
    <div className="fade-in flex h-full pt-3">
      <Box
        active={focused}
        className="flex min-w-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">设置</span><span className="text-dim">/ 阅读历史</span></>}
        titleRight={`${history.length} 条`}
      >
        <div className="grid grid-cols-[6.5em_1.3fr_1.8fr_6em_1fr] gap-3 border-b border-line px-2 pt-3 pb-1 pl-7 text-[12px] text-dim">
          <span>时间</span><span>书名</span><span>章节</span><span>时长</span><span className="hidden md:block">来源</span>
        </div>
        <div ref={ref} className="flex-1 overflow-y-auto py-1">
          {!history.length && <Empty>暂无阅读记录。</Empty>}
          {history.map((h, i) => {
            const day = h.time.split(" ")[0];
            const showDay = day !== lastDay;
            lastDay = day;
            return (
              <div key={h.id}>
                {showDay && <div className="mt-1 px-3 text-[12px] text-dim">── {day}</div>}
                <Row sel={i === sel} focused={focused} onClick={() => setSel(i)} onDoubleClick={open}>
                  <div className="grid min-w-0 flex-1 grid-cols-[6.5em_1.3fr_1.8fr_6em_1fr] items-center gap-3">
                    <span className="text-[13px] text-mute">{h.time.split(" ")[1]}</span>
                    <span className="truncate">{h.title}</span>
                    <span className="truncate text-[13px] text-mute">{h.chapter}</span>
                    <span className="text-[13px] text-info">{h.duration}</span>
                    <span className="hidden truncate text-[12px] text-dim md:block">{h.origin}</span>
                  </div>
                </Row>
              </div>
            );
          })}
        </div>
        <div className="border-t border-line px-3 py-1.5">
          {confirm ? (
            <span className="text-err">
              {confirm === "all" ? "清空全部阅读历史？" : `删除《${cur?.title}》的这条记录？`} <span className="text-hi">[y]</span> 确认 · 其他键取消
            </span>
          ) : (
            <Hints items={[["enter", "继续阅读"], ["d", "删除"], ["D", "清空"]]} />
          )}
        </div>
      </Box>
    </div>
  );
}
