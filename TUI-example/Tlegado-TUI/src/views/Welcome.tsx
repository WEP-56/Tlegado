import { useState } from "react";
import { cn } from "@/utils/cn";
import { useApp } from "@/store";
import { Box, useKeys } from "@/components/tui";
import { chapterTitle } from "@/data/mock";

const LOGO = [
  "   .·''''''·. .·''''''·.",
  "  :  ·····   :   ·····  :",
  "  :  ····    :   ·····  :",
  "  :  ·····   :   ···    :",
  "  :  ···     :   ·····  :",
  "  :  ·····   :   ····   :",
  "  '·.......·' '·.......·'",
  "         '·.___.·'",
];

export function Welcome({ focused }: { focused: boolean }) {
  const { books, sources, history, openReader, navigate, showHelp, toast } = useApp();
  const [idx, setIdx] = useState(0);
  const [sync, setSync] = useState<null | boolean>(null);

  const last = history[0] && books.find((b) => b.id === history[0].bookId);
  const updates = books.reduce((s, b) => s + b.newCount, 0);
  const okSources = sources.filter((s) => s.enabled).length;

  const menu: { label: string; key: string; run: () => void }[] = [
    { label: "打开书架", key: "b", run: () => navigate({ kind: "shelf", filter: "all" }) },
    { label: "发现 · 按书源浏览", key: "e", run: () => navigate({ kind: "discover", sourceId: sources.find((s) => s.enabled && s.enabledExplore)!.id }) },
    { label: "搜索书籍", key: "/", run: () => navigate({ kind: "search" }) },
    { label: "导入本地书籍", key: "o", run: () => toast("已扫描 ~/Books ，发现 2 本新书（演示）", "info") },
    { label: "书源管理", key: "s", run: () => navigate({ kind: "sources" }) },
    { label: "快捷键帮助", key: "?", run: showHelp },
  ];

  useKeys(focused, (e) => {
    if (e.key === "j" || e.key === "ArrowDown") setIdx((i) => Math.min(menu.length - 1, i + 1));
    else if (e.key === "k" || e.key === "ArrowUp") setIdx((i) => Math.max(0, i - 1));
    else if (e.key === "Enter") menu[idx].run();
    else if (e.key === "c" && last) openReader(last);
    else if (e.key === "b") menu[0].run();
    else if (e.key === "e") menu[1].run();
    else if (e.key === "o") menu[3].run();
    else if (e.key === "s") menu[4].run();
  });

  return (
    <div className="fade-in flex h-full flex-col overflow-y-auto px-2 pt-4">
      <Box className="px-6 py-6" active={focused}>
        <div className="flex gap-8">
          <pre className="hidden shrink-0 text-[13px] leading-[1.35] text-mute md:block">{LOGO.join("\n")}</pre>
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline gap-3">
              <span className="font-bold text-hi">Tlegado</span>
              <span className="text-dim">0.1.0-alpha</span>
            </div>
            <p className="mt-3 text-mute">
              基于 legado（阅读 3.0）书源规则的终端阅读器。书源 <span className="text-fg">{okSources}/{sources.length}</span> 可用，
              书架 <span className="text-accent">{updates}</span> 章更新待读。
            </p>
            {last && (
              <p className="mt-3">
                <button onClick={() => openReader(last)} className="cursor-pointer text-accent hover:underline">
                  [继续阅读 {last.title} · {chapterTitle(last.read)}]
                </button>
                <span className="text-dim"> or press c</span>
              </p>
            )}
            <div className="mt-4">
              {menu.map((m, i) => (
                <div
                  key={m.label}
                  onClick={() => {
                    setIdx(i);
                    m.run();
                  }}
                  onMouseEnter={() => setIdx(i)}
                  className={cn("flex cursor-pointer justify-between", i === idx && focused ? "text-hi" : "text-fg")}
                >
                  <span className="font-semibold">
                    <span className={cn("mr-1", i === idx && focused ? "text-accent" : "text-transparent")}>›</span>
                    {m.label}
                  </span>
                  <span className="text-mute">{m.key}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </Box>

      <div className="mt-6 grid gap-4 px-1 sm:grid-cols-3">
        {[
          ["今日阅读", "2小时05分", "text-hi"],
          ["本周章节", "146 章", "text-hi"],
          ["缓存占用", "38.2 MB", "text-hi"],
        ].map(([a, b, c]) => (
          <div key={a}>
            <div className="text-[12px] text-dim">{a}</div>
            <div className={c}>{b}</div>
          </div>
        ))}
      </div>

      <div className="mt-auto px-1 pt-8 pb-3">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="font-semibold text-hi">同步阅读进度</div>
            <div className="text-mute">
              默认关闭。开启后将通过 WebDAV 同步书架与阅读进度，可与安卓端「阅读」互通。
              <br />
              随时可在 <span className="underline decoration-dim underline-offset-2">设置 › 阅读偏好</span> 中修改。
            </div>
          </div>
          <div className="shrink-0 whitespace-nowrap">
            <button
              onClick={() => { setSync(false); toast("已跳过 WebDAV 同步"); }}
              className={cn("cursor-pointer", sync === false ? "text-hi" : "text-dim hover:text-fg")}
            >
              [暂不]
            </button>{" "}
            <button
              onClick={() => { setSync(true); toast("WebDAV 同步已开启（演示）", "ok"); }}
              className={cn("cursor-pointer", sync === true ? "text-ok" : "text-hi hover:text-accent")}
            >
              [开启]
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
