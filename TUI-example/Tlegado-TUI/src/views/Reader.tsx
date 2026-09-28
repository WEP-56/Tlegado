import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useApp, type ReaderState } from "@/store";
import { Bar, Box, Hints, useKeys, useScrollIntoView, useSpinner } from "@/components/tui";
import { applyPurify, chapterContent, chapterTitle, READER_THEMES, toSimplified, toTraditional } from "@/data/mock";
import { cn } from "@/utils/cn";

const FS = 16; // 阅读字号 px（一个全角字 ≈ 1em）
const NO_HEAD = "，。！？；：、”’」』）》…—";

function wrap(text: string, w: number): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < text.length) {
    let end = Math.min(text.length, i + w);
    // 标点避头：下一行首字若为标点则挤到本行尾
    while (end < text.length && NO_HEAD.includes(text[end]) && end - i < w + 2) end++;
    out.push(text.slice(i, end));
    i = end;
  }
  return out.length ? out : [""];
}

export function Reader({
  focused,
  state,
  zen,
  onClose,
  onChapter,
  onZen,
}: {
  focused: boolean;
  state: ReaderState;
  zen: boolean;
  onClose: () => void;
  onChapter: (n: number) => void;
  onZen: () => void;
}) {
  const { prefs, rules, toast, addToShelf, books } = useApp();
  const { book, chapter } = state;
  const theme = READER_THEMES[prefs.theme] ?? READER_THEMES["终端默认"];
  const lh = { "1.0": 1.6, "1.5": 2.05, "2.0": 2.5 }[prefs.spacing] ?? 2.05;

  const areaRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 600, h: 400 });
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [toc, setToc] = useState(false);
  const spin = useSpinner(loading);
  const pendingLast = useRef(false);

  useLayoutEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // 章节切换 → 模拟网络加载
  useEffect(() => {
    if (book.kind === "local") return;
    setLoading(true);
    const t = setTimeout(() => setLoading(false), 220);
    return () => clearTimeout(t);
  }, [book.id, chapter, book.kind]);

  const maxW = Math.max(12, Math.floor((size.w - 32) / FS));
  const W = prefs.width === "自适应" ? Math.min(maxW, 60) : Math.min(maxW, Number(prefs.width));
  const perPage = Math.max(3, Math.floor(size.h / (FS * lh)));

  const lines = useMemo(() => {
    let paras = chapterContent(book, chapter);
    if (prefs.purify === "开") paras = paras.map((p) => applyPurify(p, rules, book.origin).trim()).filter(Boolean);
    if (prefs.chinese === "简→繁") paras = paras.map(toTraditional);
    if (prefs.chinese === "繁→简") paras = paras.map(toSimplified);
    const ind = "　".repeat(Number(prefs.indent));
    const res: { t: string; title?: boolean }[] = [{ t: chapterTitle(chapter), title: true }, { t: "" }];
    paras.forEach((p, i) => {
      wrap(ind + p, W).forEach((t) => res.push({ t }));
      if (prefs.paraGap === "开" && i < paras.length - 1) res.push({ t: "" });
    });
    return res;
  }, [book, chapter, prefs, rules, W]);

  const scroll = prefs.pageMode === "滚动";
  const pages = scroll ? Math.max(1, lines.length - perPage + 1) : Math.max(1, Math.ceil(lines.length / perPage));
  const start = scroll ? page : page * perPage;
  const view = lines.slice(start, start + perPage);

  useEffect(() => {
    if (pendingLast.current) { setPage(pages - 1); pendingLast.current = false; }
    else setPage(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chapter, book.id]);
  useEffect(() => { if (page > pages - 1) setPage(pages - 1); }, [pages, page]);

  const goChapter = (n: number, toLast = false) => {
    if (n < 0) { toast("已经是第一章了"); return; }
    if (n >= book.total) { toast("已经是最后一章了"); return; }
    pendingLast.current = toLast;
    onChapter(n);
  };
  const next = () => (page < pages - 1 ? setPage(page + 1) : goChapter(chapter + 1));
  const prev = () => (page > 0 ? setPage(page - 1) : goChapter(chapter - 1, true));

  // 自动翻页
  useEffect(() => {
    if (prefs.autoPage === "关" || toc || !focused) return;
    const t = setInterval(next, parseInt(prefs.autoPage) * 1000);
    return () => clearInterval(t);
  });

  useKeys(focused && !toc, (e) => {
    const k = e.key;
    if (["j", "ArrowDown", " ", "ArrowRight", "l", "PageDown"].includes(k)) next();
    else if (["k", "ArrowUp", "b", "ArrowLeft", "h", "PageUp"].includes(k)) prev();
    else if (k === "]" || k === "n") goChapter(chapter + 1);
    else if (k === "[" || k === "p") goChapter(chapter - 1);
    else if (k === "t") setToc(true);
    else if (k === "f") onZen();
    else if (k === "a") addToShelf(book);
    else if (k === "q" || k === "Escape") onClose();
    else return;
    e.preventDefault();
  });

  const totalP = (chapter + (page + 1) / pages) / book.total;
  const onShelf = books.some((b) => b.title === book.title);
  const now = new Date();
  const clock = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;

  return (
    <div className={cn("fade-in relative flex h-full flex-col", !zen && "pt-3")}>
      <Box
        active={focused}
        className="flex min-h-0 flex-1 flex-col"
        title={
          <>
            <span className="font-semibold text-hi">{book.title}</span>
            <span className="text-dim">· {book.author}</span>
            {!onShelf && <span className="text-accent">· 试读</span>}
          </>
        }
        titleRight={book.kind === "local" ? `本地 ${book.format}` : book.origin}
        footer={
          <span>
            {chapter + 1}/{book.total} · {(totalP * 100).toFixed(2)}%
          </span>
        }
      >
        <div
          className="m-[3px] flex min-h-0 flex-1 flex-col rounded-[5px] transition-colors"
          style={{ background: theme.bg, color: theme.fg }}
        >
          <div ref={areaRef} className="relative min-h-0 flex-1 overflow-hidden px-4 pt-4">
            {loading ? (
              <div style={{ color: theme.dim }}>
                <span style={{ color: theme.accent }}>{spin}</span> 获取正文 ruleContent · {chapterTitle(chapter)}
              </div>
            ) : (
              <div className="mx-auto" style={{ width: `${W}em`, fontSize: FS, lineHeight: lh, fontFamily: '"Noto Sans SC", var(--font-mono)' }}>
                {view.map((l, i) => (
                  <div
                    key={start + i}
                    className={cn("whitespace-pre", l.title && "font-bold")}
                    style={{ height: `${lh}em`, color: l.title ? theme.accent : undefined }}
                  >
                    {l.t || " "}
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="flex items-center gap-3 px-4 py-1.5 text-[12px]" style={{ color: theme.dim }}>
            <span className="truncate">{chapterTitle(chapter)}</span>
            <span className="ml-auto shrink-0">
              {scroll ? `${Math.round(((page + 1) / pages) * 100)}%` : `${page + 1}/${pages} 页`}
            </span>
            {prefs.progress === "开" && (
              <span className="hidden shrink-0 sm:inline">
                <Bar value={(page + 1) / pages} width={16} />
              </span>
            )}
            {prefs.autoPage !== "关" && <span style={{ color: theme.accent }}>▶ {prefs.autoPage}</span>}
            <span className="shrink-0">{clock}</span>
          </div>
        </div>
      </Box>

      <div className="px-1 pt-2">
        <Hints items={[["space/j", "下页"], ["k", "上页"], ["[ ]", "章节"], ["t", "目录"], ["f", zen ? "退出沉浸" : "沉浸"], ...(onShelf ? [] : ([["a", "加入书架"]] as [string, string][])), ["q", "返回"]]} />
      </div>

      {toc && (
        <Toc
          total={book.total}
          current={chapter}
          latestNew={book.newCount}
          onClose={() => setToc(false)}
          onPick={(n) => { setToc(false); goChapter(n); }}
        />
      )}
    </div>
  );
}

function Toc({ total, current, latestNew, onClose, onPick }: { total: number; current: number; latestNew: number; onClose: () => void; onPick: (n: number) => void }) {
  const [sel, setSel] = useState(current);
  const ref = useScrollIntoView(sel);
  const from = Math.max(0, sel - 60);
  const to = Math.min(total, sel + 60);
  useKeys(true, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(total - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "J" || k === "PageDown") setSel((i) => Math.min(total - 1, i + 20));
    else if (k === "K" || k === "PageUp") setSel((i) => Math.max(0, i - 20));
    else if (k === "g") setSel(0);
    else if (k === "G") setSel(total - 1);
    else if (k === "Enter") onPick(sel);
    else if (k === "Escape" || k === "t" || k === "q") onClose();
    else return;
    e.preventDefault();
  });
  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/50" onClick={onClose}>
      <Box
        active
        className="flex h-[70%] w-[min(460px,90%)] flex-col bg-bg pt-2"
        title={<span className="font-semibold text-hi">目录</span>}
        titleRight={`${sel + 1}/${total}`}
        footer="J/K 翻 20 · g/G 首尾 · enter 跳转"
      >
        <div ref={ref} className="flex-1 overflow-y-auto py-1 pb-3" onClick={(e) => e.stopPropagation()}>
          {Array.from({ length: to - from }, (_, j) => from + j).map((i) => (
            <div
              key={i}
              data-sel={i === sel}
              onClick={() => onPick(i)}
              className={cn("flex cursor-pointer gap-2 px-3", i === sel ? "bg-bg3 text-hi" : "hover:bg-bg2")}
            >
              <span className={i === sel ? "text-accent" : "text-transparent"}>›</span>
              <span className={cn("flex-1 truncate", i < current && "text-dim", i === current && "text-accent")}>{chapterTitle(i)}</span>
              {i === current && <span className="text-[12px] text-accent">● 当前</span>}
              {i >= total - latestNew && <span className="text-[12px] text-ok">新</span>}
            </div>
          ))}
        </div>
      </Box>
    </div>
  );
}
