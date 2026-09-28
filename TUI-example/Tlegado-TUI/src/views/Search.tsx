import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "@/store";
import { Box, Hints, Row, useKeys, useScrollIntoView, useSpinner } from "@/components/tui";
import { searchAll, type Book } from "@/data/mock";
import { cn } from "@/utils/cn";

export function Search({ focused, focusRequest }: { focused: boolean; focusRequest: number }) {
  const { sources, books, openReader, addToShelf } = useApp();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [submitted, setSubmitted] = useState("");
  const all = useMemo(() => (submitted ? searchAll(submitted, sources) : []), [submitted, sources]);
  const [done, setDone] = useState<Record<string, boolean>>({});
  const [sel, setSel] = useState(0);
  const pending = all.filter((r) => !done[r.source.id]).length;
  const spin = useSpinner(pending > 0);

  useEffect(() => {
    setDone({});
    setSel(0);
    const ts = all.map((r, i) =>
      setTimeout(() => setDone((d) => ({ ...d, [r.source.id]: true })), 250 + i * 180 + r.source.respondTime),
    );
    return () => ts.forEach(clearTimeout);
  }, [all]);

  useEffect(() => {
    if (!focused) return;
    const t = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [focused, focusRequest]);

  const results: Book[] = all.filter((r) => done[r.source.id]).flatMap((r) => r.books);
  const cur = results[sel];
  const ref = useScrollIntoView(sel);
  const onShelf = (b: Book) => books.some((x) => x.title === b.title);

  useKeys(focused, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(results.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "Enter" && cur) openReader(cur, 0);
    else if (k === "a" && cur) addToShelf(cur);
    else if (k === "i" || k === "Enter") inputRef.current?.focus();
    else return;
    e.preventDefault();
  });

  return (
    <div className="fade-in flex h-full flex-col pt-3">
      <Box
        active={focused}
        className="flex min-h-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">发现</span><span className="text-dim">/ 搜索书籍</span></>}
        titleRight={pending ? <span className="text-accent">{spin} {all.length - pending}/{all.length} 书源</span> : `${results.length} 条结果`}
      >
        <div className="border-b border-line px-3 pt-3 pb-2">
          <div className={cn("flex items-center gap-2 rounded-[5px] border px-3 py-1.5", focused ? "border-line2" : "border-line")}>
            <span className={focused ? "text-accent" : "text-mute"}>❯</span>
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  setSubmitted(query.trim());
                  inputRef.current?.blur();
                }
                if (e.key === "Escape") {
                  e.preventDefault();
                  inputRef.current?.blur();
                }
              }}
              placeholder="输入书名或作者，按 Enter 跨书源搜索"
              className="min-w-0 flex-1 bg-transparent text-hi caret-accent outline-none placeholder:text-dim"
              spellCheck={false}
            />
            <span className="hidden shrink-0 text-[12px] text-dim sm:inline">enter 搜索</span>
          </div>
          {submitted && (
            <div className="mt-2 text-[13px]">
              <div>
                <span className={pending ? "text-accent" : "text-ok"}>●</span> <span className="font-semibold text-hi">SearchBook</span>
                <span className="text-mute">(key="{submitted}", sources={all.length})</span>
              </div>
              {all.map((r) => (
                <div key={r.source.id} className="flex gap-2 pl-2 text-mute">
                  <span className="text-dim">⎿</span>
                  <span className="w-[8em] truncate">{r.source.bookSourceName}</span>
                  {done[r.source.id] ? (
                    <>
                      <span className={r.books.length ? "text-ok" : "text-dim"}>{r.books.length ? `✓ ${r.books.length} 条` : "- 无结果"}</span>
                      <span className="text-dim">{r.source.respondTime}ms</span>
                    </>
                  ) : (
                    <span className="text-accent">{spin} 请求中…</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        <div ref={ref} className="flex-1 overflow-y-auto py-1">
          {!submitted && <div className="px-4 py-4 text-dim">输入关键词后按 Enter 搜索。试试 “剑来”、“远瞳”、“诡秘”。</div>}
          {!!submitted && !pending && !results.length && <div className="px-4 py-4 text-dim">没有找到相关书籍。试试更短的书名或作者名。</div>}
          {results.map((b, i) => (
            <Row key={b.id} sel={i === sel} focused={focused} onClick={() => setSel(i)} onDoubleClick={() => openReader(b, 0)}>
              <div className="grid min-w-0 flex-1 grid-cols-[1.5fr_1fr_1fr_1.6fr] items-center gap-3">
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate">{b.title}</span>
                  {onShelf(b) && <span className="text-[11px] text-ok">✓</span>}
                </span>
                <span className="truncate text-mute">{b.author}</span>
                <span className="truncate text-[12px] text-info">{b.origin}</span>
                <span className={cn("hidden truncate text-[13px] text-mute md:block")}>{b.latest}</span>
              </div>
            </Row>
          ))}
        </div>
        <div className="border-t border-line px-3 py-1.5">
          <Hints items={[["i /", "输入关键词"], ["enter", "试读"], ["a", "加入书架"], ["j/k", "移动"]]} />
        </div>
      </Box>
    </div>
  );
}
