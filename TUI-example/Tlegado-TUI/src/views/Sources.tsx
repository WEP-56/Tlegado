import { useEffect, useMemo, useState } from "react";
import { useApp } from "@/store";
import { Box, Hints, Row, Tabs, Toggle, useKeys, useScrollIntoView, useSpinner } from "@/components/tui";
import type { BookSource } from "@/data/mock";

const FILTERS = ["全部", "正版", "聚合", "已启用", "失效"];

export function Sources({ focused }: { focused: boolean }) {
  const { sources, setSources, toast } = useApp();
  const [filter, setFilter] = useState(0);
  const [sel, setSel] = useState(0);
  const [testing, setTesting] = useState<Set<string>>(new Set());
  const spin = useSpinner(testing.size > 0);

  const list = useMemo(
    () =>
      sources.filter((s) => {
        const f = FILTERS[filter];
        if (f === "全部") return true;
        if (f === "已启用") return s.enabled;
        if (f === "失效") return s.respondTime < 0;
        return s.bookSourceGroup === f;
      }),
    [sources, filter],
  );
  useEffect(() => { if (sel >= list.length) setSel(Math.max(0, list.length - 1)); }, [list.length, sel]);
  const cur = list[sel];
  const ref = useScrollIntoView(sel);

  const patch = (id: string, p: Partial<BookSource>) => setSources((ss) => ss.map((s) => (s.id === id ? { ...s, ...p } : s)));

  const test = (ids: string[]) => {
    setTesting((t) => new Set([...t, ...ids]));
    ids.forEach((id, i) => {
      setTimeout(() => {
        const fail = id === "s8";
        patch(id, { respondTime: fail ? -1 : 120 + Math.floor(Math.random() * 600) });
        setTesting((t) => { const n = new Set(t); n.delete(id); return n; });
        if (ids.length === 1) toast(fail ? "校验失败：连接超时" : "校验通过：搜索 / 详情 / 目录 / 正文 ✓", fail ? "err" : "ok");
      }, 500 + i * 260 + Math.random() * 400);
    });
  };

  useKeys(focused, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(list.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "h" || k === "ArrowLeft") { setFilter((f) => (f - 1 + FILTERS.length) % FILTERS.length); setSel(0); }
    else if (k === "l" || k === "ArrowRight") { setFilter((f) => (f + 1) % FILTERS.length); setSel(0); }
    else if ((k === " " || k === "Enter") && cur) { patch(cur.id, { enabled: !cur.enabled }); toast(`${cur.enabled ? "已禁用" : "已启用"} ${cur.bookSourceName}`); }
    else if (k === "e" && cur) patch(cur.id, { enabledExplore: !cur.enabledExplore });
    else if (k === "t" && cur) test([cur.id]);
    else if (k === "T") test(list.map((s) => s.id));
    else if (k === "i") toast("从剪贴板导入书源：识别到 3 个书源（演示）", "info");
    else return;
    e.preventDefault();
  });

  const json = cur && {
    bookSourceName: cur.bookSourceName,
    bookSourceGroup: cur.bookSourceGroup,
    bookSourceUrl: cur.bookSourceUrl,
    bookSourceType: 0,
    enabled: cur.enabled,
    enabledExplore: cur.enabledExplore,
    exploreUrl: cur.explore.map((e) => `${e}::/explore/${encodeURIComponent(e)}/{{page}}`).join("\n") || "",
    searchUrl: cur.searchUrl,
    ruleSearch: { bookList: ".result-list li", name: "h3@text", author: ".author@text", bookUrl: "a@href" },
    ruleBookInfo: { intro: ".intro@text", lastChapter: ".latest@text" },
    ruleToc: { chapterList: "#list dd a", chapterName: "text", chapterUrl: "href" },
    ruleContent: { content: "#content@html", replaceRegex: "##本章未完.*" },
  };

  return (
    <div className="fade-in flex h-full gap-3 pt-3">
      <Box
        active={focused}
        className="flex min-w-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">设置</span><span className="text-dim">/ 书源管理</span></>}
        titleRight={testing.size ? <span className="text-accent">{spin} 校验中 {testing.size}</span> : `${sources.filter((s) => s.enabled).length}/${sources.length} 启用`}
      >
        <div className="px-3 pt-3 pb-1">
          <Tabs items={FILTERS} index={filter} onChange={(i) => { setFilter(i); setSel(0); }} focused={focused} />
        </div>
        <div className="grid grid-cols-[2.2em_1.4fr_0.6fr_2fr_0.8fr_0.6fr] gap-3 border-b border-line px-2 pb-1 pl-7 text-[12px] text-dim">
          <span>启用</span><span>书源名</span><span>分组</span><span className="hidden md:block">地址</span><span>响应</span><span>发现</span>
        </div>
        <div ref={ref} className="flex-1 overflow-y-auto py-1">
          {list.map((s, i) => (
            <Row key={s.id} sel={i === sel} focused={focused} onClick={() => setSel(i)}>
              <div className="grid min-w-0 flex-1 grid-cols-[2.2em_1.4fr_0.6fr_2fr_0.8fr_0.6fr] items-center gap-3">
                <span onClick={(e) => { e.stopPropagation(); patch(s.id, { enabled: !s.enabled }); }}><Toggle on={s.enabled} /></span>
                <span className={s.enabled ? "truncate" : "truncate text-dim line-through decoration-line2"}>{s.bookSourceName}</span>
                <span className="text-[12px] text-info">{s.bookSourceGroup}</span>
                <span className="hidden truncate text-[12px] text-dim md:block">{s.bookSourceUrl}</span>
                <span className="text-[12px]">
                  {testing.has(s.id) ? (
                    <span className="text-accent">{spin}</span>
                  ) : s.respondTime < 0 ? (
                    <span className="text-err">✗ 超时</span>
                  ) : (
                    <span className={s.respondTime > 500 ? "text-accent" : "text-ok"}>{s.respondTime}ms</span>
                  )}
                </span>
                <span className="text-[12px]">{s.enabledExplore && s.explore.length ? <span className="text-ok">●</span> : <span className="text-dim">○</span>}</span>
              </div>
            </Row>
          ))}
        </div>
        <div className="border-t border-line px-3 py-1.5">
          <Hints items={[["space", "启用/禁用"], ["e", "发现开关"], ["t", "校验"], ["T", "全部校验"], ["i", "导入"], ["h/l", "筛选"]]} />
        </div>
      </Box>

      {json && (
        <Box className="hidden w-[380px] shrink-0 flex-col xl:flex" title={<span className="text-mute">BookSource.json</span>} titleRight="只读预览">
          <pre className="flex-1 overflow-auto px-3 pt-4 pb-3 text-[12px] leading-[1.6]">
            <Json v={json} />
          </pre>
        </Box>
      )}
    </div>
  );
}

function Json({ v, ind = 0 }: { v: unknown; ind?: number }): React.ReactElement {
  const pad = "  ".repeat(ind);
  if (typeof v === "string") return <span className="text-ok">{JSON.stringify(v)}</span>;
  if (typeof v === "number") return <span className="text-mag">{v}</span>;
  if (typeof v === "boolean") return <span className="text-accent">{String(v)}</span>;
  if (v && typeof v === "object") {
    const entries = Object.entries(v as Record<string, unknown>);
    return (
      <>
        <span className="text-dim">{"{"}</span>
        {"\n"}
        {entries.map(([k, val], i) => (
          <span key={k}>
            {pad}  <span className="text-info">"{k}"</span><span className="text-dim">: </span>
            <Json v={val} ind={ind + 1} />
            {i < entries.length - 1 && <span className="text-dim">,</span>}
            {"\n"}
          </span>
        ))}
        {pad}<span className="text-dim">{"}"}</span>
      </>
    );
  }
  return <span className="text-dim">null</span>;
}
