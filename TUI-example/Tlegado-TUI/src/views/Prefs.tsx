import { useState } from "react";
import { useApp } from "@/store";
import { Box, Hints, useKeys, useScrollIntoView } from "@/components/tui";
import { DEFAULT_PREFS, PREF_DEFS, READER_THEMES } from "@/data/mock";
import { cn } from "@/utils/cn";

export function Prefs({ focused }: { focused: boolean }) {
  const { prefs, setPrefs, toast } = useApp();
  const [sel, setSel] = useState(0);
  const ref = useScrollIntoView(sel);

  const cycle = (i: number, d: number) => {
    const def = PREF_DEFS[i];
    const cur = def.options.indexOf(prefs[def.key]);
    const next = def.options[(cur + d + def.options.length) % def.options.length];
    setPrefs((p) => ({ ...p, [def.key]: next }));
  };

  useKeys(focused, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(PREF_DEFS.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if (k === "h" || k === "ArrowLeft") cycle(sel, -1);
    else if (k === "l" || k === "ArrowRight" || k === " " || k === "Enter") cycle(sel, 1);
    else if (k === "R") { setPrefs(DEFAULT_PREFS); toast("已恢复默认阅读偏好", "info"); }
    else return;
    e.preventDefault();
  });

  const theme = READER_THEMES[prefs.theme];
  const lh = { "1.0": 1.6, "1.5": 2.05, "2.0": 2.5 }[prefs.spacing] ?? 2;
  const ind = "　".repeat(Number(prefs.indent));
  let lastSec = "";

  return (
    <div className="fade-in flex h-full gap-3 pt-3">
      <Box
        active={focused}
        className="flex min-w-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">设置</span><span className="text-dim">/ 阅读偏好</span></>}
        titleRight="~/.config/tlegado/config.toml"
      >
        <div ref={ref} className="flex-1 overflow-y-auto pt-2 pb-2">
          {PREF_DEFS.map((d, i) => {
            const head = d.section !== lastSec;
            lastSec = d.section;
            const s = i === sel;
            return (
              <div key={d.key}>
                {head && <div className="mt-2 px-3 text-[12px] text-dim">── {d.section}</div>}
                <div
                  data-sel={s}
                  onClick={() => setSel(i)}
                  className={cn("flex cursor-pointer items-center gap-2 px-2", s && focused ? "bg-bg3" : s ? "bg-bg2" : "hover:bg-bg2")}
                >
                  <span className={cn("w-3", s ? (focused ? "text-accent" : "text-dim") : "text-transparent")}>›</span>
                  <span className={cn("w-[8em] shrink-0", s && "text-hi")}>{d.label}</span>
                  <span className="flex flex-1 flex-wrap items-center gap-x-1">
                    {d.options.map((o) => (
                      <button
                        key={o}
                        onClick={(e) => { e.stopPropagation(); setSel(i); setPrefs((p) => ({ ...p, [d.key]: o })); }}
                        className={cn("cursor-pointer px-0.5 text-[13px]", prefs[d.key] === o ? "text-accent" : "text-dim hover:text-fg")}
                      >
                        {prefs[d.key] === o ? `‹${o}›` : ` ${o} `}
                      </button>
                    ))}
                  </span>
                  <span className="hidden truncate text-[12px] text-dim lg:block lg:w-[14em]">{d.desc}</span>
                </div>
              </div>
            );
          })}
        </div>
        <div className="border-t border-line px-3 py-1.5">
          <Hints items={[["j/k", "移动"], ["h/l", "切换选项"], ["R", "恢复默认"]]} />
        </div>
      </Box>

      <Box className="hidden w-[340px] shrink-0 flex-col xl:flex" title={<span className="text-mute">实时预览</span>} titleRight={prefs.theme}>
        <div className="m-[3px] mt-3 flex-1 overflow-hidden rounded-[5px] px-4 py-4" style={{ background: theme.bg, color: theme.fg, fontSize: 15, lineHeight: lh, fontFamily: '"Noto Sans SC", var(--font-mono)' }}>
          <div className="font-bold" style={{ color: theme.accent }}>第一章 雾中来客</div>
          <div style={{ height: `${lh}em` }} />
          {["夜色像一张被墨水浸透的旧纸，缓慢地铺满了整条长街。", "他站在书店门口，指尖还残留着纸页的触感。", "“你确定要打开它？”身后传来低低的声音。"].map((t, i) => (
            <div key={i} style={{ marginBottom: prefs.paraGap === "开" ? `${lh}em` : 0 }}>{ind}{t}</div>
          ))}
          {prefs.progress === "开" && (
            <div className="mt-3 text-[12px]" style={{ color: theme.dim }}>
              第一章 雾中来客 · 1/6 页 <span style={{ color: theme.accent }}>{"━".repeat(3)}</span>{"━".repeat(9)}
            </div>
          )}
        </div>
      </Box>
    </div>
  );
}
