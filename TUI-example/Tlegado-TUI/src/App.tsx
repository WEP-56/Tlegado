import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Ctx, type AppCtx, type ReaderState, type Route } from "@/store";
import {
  DEFAULT_PREFS, HISTORY, PURIFY_RULES, SHELF, SOURCES, chapterTitle,
  type Book, type HistoryItem, type Prefs,
} from "@/data/mock";
import { Sidebar, type NavItem } from "@/components/Sidebar";
import { Help } from "@/components/Help";
import { Welcome } from "@/views/Welcome";
import { Bookshelf } from "@/views/Bookshelf";
import { Discover } from "@/views/Discover";
import { Reader } from "@/views/Reader";
import { History } from "@/views/History";
import { Sources } from "@/views/Sources";
import { Purify } from "@/views/Purify";
import { Prefs as PrefsView } from "@/views/Prefs";
import { Search } from "@/views/Search";
import { cn } from "@/utils/cn";

type Focus = "sidebar" | "main";

export default function App() {
  const [books, setBooks] = useState<Book[]>(SHELF);
  const [sources, setSources] = useState(SOURCES);
  const [rules, setRules] = useState(PURIFY_RULES);
  const [prefs, setPrefs] = useState<Prefs>(DEFAULT_PREFS);
  const [history, setHistory] = useState<HistoryItem[]>(HISTORY);

  const [navId, setNavId] = useState("home");
  const [focus, setFocus] = useState<Focus>("sidebar");
  const [reader, setReader] = useState<ReaderState | null>(null);
  const [searchFocus, setSearchFocus] = useState(0);
  const [zen, setZen] = useState(false);
  const [sidebarHidden, setSidebarHidden] = useState(false);
  const [help, setHelp] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [toastMsg, setToastMsg] = useState<{ msg: string; tone: string; id: number } | null>(null);

  // ── 侧栏导航项 ───────────────────────────────────────────
  const nav: NavItem[] = useMemo(() => {
    const updates = books.reduce((s, b) => s + b.newCount, 0);
    const local = books.filter((b) => b.kind === "local").length;
    const items: NavItem[] = [
      { id: "home", section: null, label: "首页", route: { kind: "home" } },
      { id: "shelf:all", section: "书架", label: "全部书籍", route: { kind: "shelf", filter: "all" }, right: books.length },
      { id: "shelf:local", section: "书架", label: "本地图书", route: { kind: "shelf", filter: "local" }, right: local },
      {
        id: "shelf:network", section: "书架", label: "网络图书", route: { kind: "shelf", filter: "network" },
        right: <>{updates > 0 && <span className="text-accent">+{updates} </span>}{books.length - local}</>,
      },
      { id: "discover:search", section: "发现", label: "搜索书籍", route: { kind: "search" }, right: "/" },
    ];
    sources
      .filter((s) => s.enabled && s.enabledExplore && s.explore.length)
      .forEach((s) =>
        items.push({
          id: "discover:" + s.id, section: "发现", label: s.bookSourceName, route: { kind: "discover", sourceId: s.id },
          right: <span className={s.respondTime > 500 ? "text-accent/80" : ""}>{s.explore.length}类</span>,
        }),
      );
    items.push(
      { id: "set:history", section: "设置", label: "阅读历史", route: { kind: "history" }, right: history.length },
      { id: "set:sources", section: "设置", label: "书源管理", route: { kind: "sources" }, right: `${sources.filter((s) => s.enabled).length}/${sources.length}` },
      { id: "set:purify", section: "设置", label: "净化规则", route: { kind: "purify" }, right: rules.filter((r) => r.enabled).length },
      { id: "set:prefs", section: "设置", label: "阅读偏好", route: { kind: "prefs" } },
    );
    return items;
  }, [books, sources, history.length, rules]);

  const navIdx = Math.max(0, nav.findIndex((n) => n.id === navId));
  const route: Route = nav[navIdx].route;
  const visibleIdx = nav.map((n, i) => (n.section && collapsed[n.section] ? -1 : i)).filter((i) => i >= 0);

  // ── 动作 ─────────────────────────────────────────────────
  const toast = useCallback((msg: string, tone: "ok" | "err" | "info" = "info") => {
    setToastMsg({ msg, tone, id: Date.now() });
  }, []);
  useEffect(() => {
    if (!toastMsg) return;
    const t = setTimeout(() => setToastMsg(null), 2800);
    return () => clearTimeout(t);
  }, [toastMsg]);

  const navigate = useCallback((r: Route) => {
    const id =
      r.kind === "home" ? "home"
      : r.kind === "shelf" ? "shelf:" + r.filter
      : r.kind === "search" ? "discover:search"
      : r.kind === "discover" ? "discover:" + r.sourceId
      : "set:" + r.kind;
    setNavId(id);
    setReader(null);
    setZen(false);
    setFocus("main");
  }, []);

  const openSearch = useCallback(() => {
    setNavId("discover:search");
    setReader(null);
    setZen(false);
    setFocus("main");
    setSearchFocus((n) => n + 1);
  }, []);

  const openReader = useCallback((book: Book, chapter?: number) => {
    setReader({ book, chapter: chapter ?? book.read });
    setFocus("main");
  }, []);

  const onChapter = (n: number) => {
    if (!reader) return;
    const b = reader.book;
    setReader({ book: b, chapter: n });
    setBooks((bs) =>
      bs.map((x) => (x.id === b.id ? { ...x, read: n, lastRead: "刚刚", newCount: Math.min(x.newCount, Math.max(0, x.total - 1 - n)) } : x)),
    );
    setHistory((h) => [
      {
        id: "h" + Date.now(), bookId: b.id, title: b.title, author: b.author, chapter: chapterTitle(n), chapterIndex: n,
        time: "今天 " + new Date().toTimeString().slice(0, 5), duration: "进行中", origin: b.kind === "local" ? `本地 ${b.format}` : b.origin!,
      },
      ...h.filter((x) => x.bookId !== b.id),
    ]);
  };

  const addToShelf = useCallback((book: Book) => {
    setBooks((bs) => {
      if (bs.some((b) => b.title === book.title)) {
        setTimeout(() => toast(`《${book.title}》已在书架中`), 0);
        return bs;
      }
      setTimeout(() => toast(`已将《${book.title}》加入书架`, "ok"), 0);
      return [{ ...book, group: book.status === "连载" ? "追更" : "完结", lastRead: "未读" }, ...bs];
    });
  }, [toast]);

  // ── 全局按键 ─────────────────────────────────────────────
  const globalKey = useRef<(e: KeyboardEvent) => void>(() => {});
  globalKey.current = (e: KeyboardEvent) => {
    const inInput = (e.target as HTMLElement)?.tagName === "INPUT";
    if (e.ctrlKey && e.key.toLowerCase() === "b") { e.preventDefault(); setSidebarHidden((h) => !h); return; }
    if (inInput) return;
    if (help) {
      if (e.key === "Escape" || e.key === "?" || e.key === "q") { e.preventDefault(); setHelp(false); }
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      if (sidebarHidden || zen) return setFocus("main");
      setFocus((f) => (f === "sidebar" ? "main" : "sidebar"));
      return;
    }
    if (e.key === "/") { e.preventDefault(); openSearch(); return; }
    if (e.key === "?") { setHelp(true); return; }

    if (focus === "sidebar") {
      const pos = visibleIdx.indexOf(navIdx);
      const move = (d: number) => {
        const np = Math.max(0, Math.min(visibleIdx.length - 1, pos + d));
        setNavId(nav[visibleIdx[np]].id);
        setReader(null); setZen(false);
      };
      if (e.key === "j" || e.key === "ArrowDown") { e.preventDefault(); move(1); }
      else if (e.key === "k" || e.key === "ArrowUp") { e.preventDefault(); move(-1); }
      else if (e.key === "g") move(-999);
      else if (e.key === "G") move(999);
      else if (e.key === "Enter" || e.key === "l" || e.key === "ArrowRight") { e.preventDefault(); setFocus("main"); }
      else if ((e.key === "h" || e.key === "ArrowLeft" || e.key === " ") && nav[navIdx].section) {
        e.preventDefault();
        const sec = nav[navIdx].section!;
        setCollapsed((c) => ({ ...c, [sec]: !c[sec] }));
        if (!collapsed[sec]) setNavId(nav.find((n) => n.section === sec)!.id);
      }
      return;
    }
    if (focus === "main" && e.key === "Escape" && !reader) { setFocus("sidebar"); }
  };
  useEffect(() => {
    const fn = (e: KeyboardEvent) => globalKey.current(e);
    window.addEventListener("keydown", fn);
    return () => window.removeEventListener("keydown", fn);
  }, []);

  const ctx: AppCtx = {
    books, setBooks, sources, setSources, rules, setRules, prefs, setPrefs, history, setHistory,
    openReader, addToShelf, toast, navigate, showHelp: () => setHelp(true),
  };

  const mainFocused = focus === "main" && !help;
  const showSidebar = !sidebarHidden && !(zen && reader);

  let main;
  if (reader) {
    main = (
      <Reader
        key={reader.book.id}
        focused={mainFocused}
        state={reader}
        zen={zen}
        onZen={() => setZen((z) => !z)}
        onClose={() => { setReader(null); setZen(false); }}
        onChapter={onChapter}
      />
    );
  } else {
    switch (route.kind) {
      case "home": main = <Welcome focused={mainFocused} />; break;
      case "shelf": main = <Bookshelf focused={mainFocused} filter={route.filter} />; break;
      case "search": main = <Search focused={mainFocused} focusRequest={searchFocus} />; break;
      case "discover": main = <Discover key={route.sourceId} focused={mainFocused} sourceId={route.sourceId} />; break;
      case "history": main = <History focused={mainFocused} />; break;
      case "sources": main = <Sources focused={mainFocused} />; break;
      case "purify": main = <Purify focused={mainFocused} />; break;
      case "prefs": main = <PrefsView focused={mainFocused} />; break;
    }
  }

  const crumb = reader ? `阅读 › ${reader.book.title}` : nav[navIdx].section ? `${nav[navIdx].section} › ${nav[navIdx].label}` : "首页";
  const toneCls = toastMsg?.tone === "ok" ? "text-ok" : toastMsg?.tone === "err" ? "text-err" : "text-info";
  const toneIcon = toastMsg?.tone === "ok" ? "✓" : toastMsg?.tone === "err" ? "✗" : "●";

  return (
    <Ctx.Provider value={ctx}>
      <div className="flex h-full flex-col px-4 pt-3 pb-2 sm:px-5">
        {/* 顶栏 */}
        <div className="flex items-center justify-between text-[13px]">
          <span className="text-mute">
            ~/.tlegado <span className="text-dim">›</span> <span className="text-fg">{crumb}</span>
          </span>
          <span className="text-dim">
            <span className="hidden sm:inline">{focus === "sidebar" ? "侧栏" : "主体"} · </span>Tlegado 0.1.0
          </span>
        </div>

        {/* 主区域 */}
        <div className="mt-3 flex min-h-0 flex-1 gap-3">
          {showSidebar && (
            <div className="w-[230px] shrink-0 pt-3" onClick={() => setFocus("sidebar")}>
              <Sidebar
                items={nav}
                index={navIdx}
                focused={focus === "sidebar" && !help}
                collapsed={collapsed}
                onSelect={(i) => { setNavId(nav[i].id); setReader(null); setZen(false); setFocus("sidebar"); }}
              />
            </div>
          )}
          <div className="min-w-0 flex-1" onMouseDown={() => focus !== "main" && setFocus("main")}>
            {main}
          </div>
        </div>

        {/* 提示行 */}
        <div className="mt-2 flex items-center justify-between gap-4 text-[12px]">
          <div className="min-w-0 truncate">
            {toastMsg ? (
              <span key={toastMsg.id} className={cn("fade-in", toneCls)}>{toneIcon} {toastMsg.msg}</span>
            ) : (
              <span className="text-dim">
                <span className="text-mute">tab</span> 切换焦点 · <span className="text-mute">j/k</span> 移动 · <span className="text-mute">enter</span> 打开 ·{" "}
                <span className="text-mute">/</span> 搜索 · <span className="text-mute">ctrl+b</span> 侧栏 · <span className="text-mute">?</span> 帮助
              </span>
            )}
          </div>
          <span className="shrink-0 text-dim">[alpha]</span>
        </div>
      </div>
      {help && <Help onClose={() => setHelp(false)} />}
    </Ctx.Provider>
  );
}
