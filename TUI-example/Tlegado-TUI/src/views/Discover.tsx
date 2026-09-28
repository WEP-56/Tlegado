import { useEffect, useState } from "react";
import { useApp } from "@/store";
import { Box, Empty, Hints, Row, Tabs, useKeys, useScrollIntoView, useSpinner } from "@/components/tui";
import { getDiscoverBooks, type Book } from "@/data/mock";
import { BookDetail } from "./Bookshelf";

export function Discover({ focused, sourceId }: { focused: boolean; sourceId: string }) {
  const { sources, books, openReader, addToShelf } = useApp();
  const source = sources.find((s) => s.id === sourceId)!;
  const [cat, setCat] = useState(0);
  const [sel, setSel] = useState(0);
  const [loading, setLoading] = useState(true);
  const [list, setList] = useState<Book[]>([]);
  const spin = useSpinner(loading);

  useEffect(() => { setCat(0); }, [sourceId]);

  useEffect(() => {
    if (!source.explore.length) { setLoading(false); setList([]); return; }
    setLoading(true);
    setSel(0);
    const t = setTimeout(() => {
      setList(getDiscoverBooks(source, source.explore[cat] ?? source.explore[0]));
      setLoading(false);
    }, 260 + source.respondTime * 0.6);
    return () => clearTimeout(t);
  }, [source, cat]);

  const scrollRef = useScrollIntoView(sel);
  const cur = list[sel];
  const onShelf = (b: Book) => books.some((x) => x.title === b.title);

  useKeys(focused, (e) => {
    const k = e.key;
    const n = source.explore.length;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(list.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if ((k === "h" || k === "ArrowLeft") && n) setCat((i) => (i - 1 + n) % n);
    else if ((k === "l" || k === "ArrowRight") && n) setCat((i) => (i + 1) % n);
    else if (k === "Enter" && cur) openReader(cur, 0);
    else if (k === "a" && cur) addToShelf(cur);
    else return;
    e.preventDefault();
  });

  const lat = source.respondTime < 0 ? "超时" : `${source.respondTime}ms`;

  return (
    <div className="fade-in flex h-full gap-3 pt-3">
      <Box
        active={focused}
        className="flex min-w-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">发现</span><span className="text-dim">/ {source.bookSourceName}</span></>}
        titleRight={<span>{source.bookSourceUrl} · <span className={source.respondTime > 500 ? "text-accent" : "text-ok"}>{lat}</span></span>}
      >
        <div className="px-3 pt-3 pb-2">
          <Tabs items={source.explore} index={cat} onChange={setCat} focused={focused} />
        </div>
        <div className="grid grid-cols-[1.5fr_1fr_0.7fr_0.7fr_1.6fr] gap-3 border-b border-line px-2 pb-1 pl-7 text-[12px] text-dim">
          <span>书名</span><span>作者</span><span>分类</span><span>状态</span><span className="hidden md:block">最新章节</span>
        </div>

        <div ref={scrollRef} className="flex-1 overflow-y-auto py-1">
          {loading && (
            <div className="px-4 py-3 text-mute">
              <span className="text-accent">{spin}</span> 正在请求 {source.bookSourceUrl}/explore/{source.explore[cat]} …
              <div className="mt-1 text-[12px] text-dim">  ↳ 解析规则 ruleExplore.bookList</div>
            </div>
          )}
          {!loading && !source.explore.length && <Empty>该书源未配置发现规则（exploreUrl 为空）。</Empty>}
          {!loading &&
            list.map((b, i) => (
              <Row key={b.id} sel={i === sel} focused={focused} onClick={() => setSel(i)} onDoubleClick={() => openReader(b, 0)}>
                <div className="grid min-w-0 flex-1 grid-cols-[1.5fr_1fr_0.7fr_0.7fr_1.6fr] items-center gap-3">
                  <span className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate">{b.title}</span>
                    {onShelf(b) && <span className="shrink-0 text-[11px] text-ok">✓</span>}
                  </span>
                  <span className="truncate text-mute">{b.author}</span>
                  <span className="text-[12px] text-info">{b.category}</span>
                  <span className={b.status === "完结" ? "text-[12px] text-ok" : "text-[12px] text-accent"}>{b.status}</span>
                  <span className="hidden truncate text-[13px] text-mute md:block">{b.latest}</span>
                </div>
              </Row>
            ))}
        </div>

        <div className="border-t border-line px-3 py-1.5">
          <Hints items={[["enter", "试读"], ["a", "加入书架"], ["h/l", "切换分类"], ["j/k", "移动"]]} />
        </div>
      </Box>
      {cur && !loading && (
        <BookDetail
          book={cur}
          extra={
            <button
              onClick={() => addToShelf(cur)}
              className="mt-4 cursor-pointer text-accent hover:underline"
            >
              {onShelf(cur) ? <span className="text-ok">[✓ 已在书架]</span> : "[+ 加入书架]"} <span className="text-dim">a</span>
            </button>
          }
        />
      )}
    </div>
  );
}
