import { useEffect, useMemo, useState } from "react";
import { useApp } from "@/store";
import { Bar, Box, Empty, Hints, Row, Tabs, useKeys, useScrollIntoView, useSpinner } from "@/components/tui";
import { chapterTitle, type Book } from "@/data/mock";
import { cn } from "@/utils/cn";

const SORTS = ["最近阅读", "书名", "更新数", "进度"];

export function Bookshelf({ focused, filter }: { focused: boolean; filter: "all" | "local" | "network" }) {
  const { books, setBooks, openReader, toast } = useApp();
  const [group, setGroup] = useState(0);
  const [sel, setSel] = useState(0);
  const [sort, setSort] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);
  const spin = useSpinner(refreshing);

  const base = books.filter((b) => filter === "all" || b.kind === filter);
  const groups = useMemo(() => ["全部", ...Array.from(new Set(base.map((b) => b.group)))], [base]);
  const g = groups[Math.min(group, groups.length - 1)];

  const list = useMemo(() => {
    const l = base.filter((b) => g === "全部" || b.group === g);
    if (sort === 1) l.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    if (sort === 2) l.sort((a, b) => b.newCount - a.newCount);
    if (sort === 3) l.sort((a, b) => b.read / b.total - a.read / a.total);
    return l;
  }, [base, g, sort]);

  useEffect(() => { setSel(0); setGroup(0); }, [filter]);
  useEffect(() => { if (sel >= list.length) setSel(Math.max(0, list.length - 1)); }, [list.length, sel]);
  useEffect(() => setConfirmDel(false), [sel]);

  const cur: Book | undefined = list[sel];
  const scrollRef = useScrollIntoView(sel);

  const refresh = () => {
    if (refreshing) return;
    setRefreshing(true);
    setTimeout(() => {
      setRefreshing(false);
      setBooks((bs) => bs.map((b) => (b.kind === "network" && b.status === "连载" ? { ...b, newCount: b.newCount + 1, total: b.total + 1, latest: chapterTitle(b.total) } : b)));
      toast("检查更新完成：3 本书有新章节", "ok");
    }, 1400);
  };

  useKeys(focused, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(list.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "g") setSel(0);
    else if (k === "G") setSel(list.length - 1);
    else if (k === "h" || k === "ArrowLeft") { setGroup((i) => (i - 1 + groups.length) % groups.length); setSel(0); }
    else if (k === "l" || k === "ArrowRight") { setGroup((i) => (i + 1) % groups.length); setSel(0); }
    else if (k === "Enter" && cur) openReader(cur);
    else if (k === "s") setSort((s) => (s + 1) % SORTS.length);
    else if (k === "r") refresh();
    else if (k === "x" && cur) {
      if (!confirmDel) { setConfirmDel(true); return; }
      setBooks((bs) => bs.filter((b) => b.id !== cur.id));
      toast(`已将《${cur.title}》移出书架`, "err");
      setConfirmDel(false);
    } else return;
    e.preventDefault();
  });

  const title = { all: "全部书籍", local: "本地图书", network: "网络图书" }[filter];

  return (
    <div className="fade-in flex h-full gap-3 pt-3">
      <Box
        active={focused}
        className="flex min-w-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">书架</span><span className="text-dim">/ {title}</span></>}
        titleRight={refreshing ? <span className="text-accent">{spin} 检查更新中…</span> : `${list.length} 本`}
      >
        <div className="flex items-center justify-between gap-2 px-3 pt-3 pb-1">
          <Tabs items={groups} index={groups.indexOf(g)} onChange={(i) => { setGroup(i); setSel(0); }} focused={focused} />
          <button onClick={() => setSort((s) => (s + 1) % SORTS.length)} className="cursor-pointer text-[12px] text-dim hover:text-fg">
            排序: <span className="text-mute">{SORTS[sort]}</span> ↓
          </button>
        </div>

        <div className="grid grid-cols-[1.6fr_1fr_1.3fr_1.8fr_1fr] gap-3 border-b border-line px-2 pt-1 pb-1 pl-7 text-[12px] text-dim">
          <span>书名</span><span>作者</span><span>进度</span><span className="hidden md:block">最新章节</span><span className="hidden md:block">来源</span>
        </div>

        <div ref={scrollRef} className="flex-1 overflow-y-auto py-1">
          {list.length === 0 && <Empty>这里空空如也。按 <span className="text-fg">/</span> 输入 <span className="text-accent">/search 书名</span> 添加书籍。</Empty>}
          {list.map((b, i) => (
            <Row key={b.id} sel={i === sel} focused={focused} onClick={() => setSel(i)} onDoubleClick={() => openReader(b)}>
              <div className="grid min-w-0 flex-1 grid-cols-[1.6fr_1fr_1.3fr_1.8fr_1fr] items-center gap-3">
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate">{b.title}</span>
                  {b.newCount > 0 && <span className="shrink-0 text-[11px] text-accent">+{b.newCount}</span>}
                </span>
                <span className="truncate text-mute">{b.author}</span>
                <span className="flex items-center gap-2 text-[12px]">
                  <Bar value={(b.read + (b.read ? 1 : 0)) / b.total} width={8} color={b.read + 1 >= b.total ? "text-ok" : "text-accent"} />
                  <span className="text-dim">{Math.round(((b.read + (b.read ? 1 : 0)) / b.total) * 100)}%</span>
                </span>
                <span className="hidden truncate text-[13px] text-mute md:block">{b.latest}</span>
                <span className="hidden truncate text-[12px] md:block">
                  {b.kind === "local" ? <span className="text-info">本地·{b.format}</span> : <span className="text-dim">{b.origin}</span>}
                </span>
              </div>
            </Row>
          ))}
        </div>

        <div className="border-t border-line px-3 py-1.5">
          {confirmDel ? (
            <span className="text-err">再按一次 x 确认移出《{cur?.title}》· 其他键取消</span>
          ) : (
            <Hints items={[["enter", "阅读"], ["h/l", "分组"], ["s", "排序"], ["r", "检查更新"], ["x", "移出"]]} />
          )}
        </div>
      </Box>

      {cur && <BookDetail book={cur} />}
    </div>
  );
}

export function BookDetail({ book, extra }: { book: Book; extra?: React.ReactNode }) {
  const p = book.total ? (book.read + (book.read ? 1 : 0)) / book.total : 0;
  return (
    <Box className="hidden w-[330px] shrink-0 flex-col xl:flex" title={<span className="text-mute">详情</span>}>
      <div className="flex-1 overflow-y-auto px-4 pt-4 pb-3">
        <div className="text-[16px] font-bold text-hi">{book.title}</div>
        <div className="text-mute">{book.author}</div>
        <div className="mt-2 flex flex-wrap gap-2 text-[12px]">
          <span className="text-info">#{book.category}</span>
          <span className={book.status === "完结" ? "text-ok" : "text-accent"}>#{book.status}</span>
          <span className="text-dim">#{book.words}字</span>
          {book.kind === "local" && <span className="text-mag">#{book.format}</span>}
        </div>

        <div className="mt-4 space-y-0.5 text-[13px]">
          <Field k="来源" v={book.kind === "local" ? book.path! : book.origin!} />
          <Field k="章节" v={`${book.total} 章`} />
          <Field k="最新" v={book.latest} />
          <Field k="上次" v={book.lastRead} />
          {book.read > 0 && <Field k="读到" v={chapterTitle(book.read)} />}
        </div>

        {book.lastRead !== "未读" && (
          <div className="mt-3 flex items-center gap-2 text-[12px]">
            <Bar value={p} width={22} />
            <span className="text-dim">{(p * 100).toFixed(1)}%</span>
          </div>
        )}

        <div className="mt-4 text-[12px] text-dim">简介</div>
        <p className="mt-1 text-[13px] leading-relaxed text-fg">{book.intro}</p>

        <div className="mt-4 text-[12px] text-dim">目录预览</div>
        <div className="mt-1 space-y-0.5 text-[13px]">
          {[0, 1, 2].map((i) => (
            <div key={i} className="truncate text-mute">{chapterTitle(i)}</div>
          ))}
          <div className="text-line">⋮</div>
          <div className={cn("truncate", book.newCount ? "text-accent" : "text-mute")}>{book.latest}</div>
        </div>
        {extra}
      </div>
    </Box>
  );
}

function Field({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex gap-2">
      <span className="w-8 shrink-0 text-dim">{k}</span>
      <span className="truncate text-fg">{v}</span>
    </div>
  );
}
