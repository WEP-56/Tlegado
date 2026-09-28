import { useEffect, useState } from "react";
import { useApp } from "@/store";
import { Box, Hints, Row, Toggle, useKeys, useScrollIntoView } from "@/components/tui";
import type { PurifyRule } from "@/data/mock";

const SAMPLE =
  "钟楼的指针停在十一点五十九分。（笔趣阁 www.biquge.example 最新章节）风从巷口灌进来……本章未完，请点击下一页继续阅读 他没有回头。求月票！求推荐票！。。。***";

function ranges(text: string, r: PurifyRule): [number, number][] {
  const out: [number, number][] = [];
  try {
    if (r.isRegex) {
      for (const m of text.matchAll(new RegExp(r.pattern, "g"))) {
        if (m[0].length) out.push([m.index!, m.index! + m[0].length]);
      }
    } else {
      let i = text.indexOf(r.pattern);
      while (i >= 0 && r.pattern) { out.push([i, i + r.pattern.length]); i = text.indexOf(r.pattern, i + r.pattern.length); }
    }
  } catch { /* noop */ }
  return out;
}

export function Purify({ focused }: { focused: boolean }) {
  const { rules, setRules, toast } = useApp();
  const [sel, setSel] = useState(0);
  const ref = useScrollIntoView(sel);
  const cur = rules[sel];
  useEffect(() => { if (sel >= rules.length) setSel(Math.max(0, rules.length - 1)); }, [rules.length, sel]);

  useKeys(focused, (e) => {
    const k = e.key;
    if (k === "j" || k === "ArrowDown") setSel((i) => Math.min(rules.length - 1, i + 1));
    else if (k === "k" || k === "ArrowUp") setSel((i) => Math.max(0, i - 1));
    else if ((k === " " || k === "Enter") && cur) setRules((rs) => rs.map((r) => (r.id === cur.id ? { ...r, enabled: !r.enabled } : r)));
    else if (k === "n") {
      const id = "r" + Date.now();
      setRules((rs) => [...rs, { id, name: "新规则", pattern: "广告.{0,6}", replacement: "", isRegex: true, scope: "全部", enabled: false }]);
      setSel(rules.length);
      toast("已添加新规则（演示：编辑功能待实现）", "info");
    } else if (k === "d" && cur) { setRules((rs) => rs.filter((r) => r.id !== cur.id)); toast(`已删除规则「${cur.name}」`, "err"); }
    else if (k === "J" && cur && sel < rules.length - 1) { setRules((rs) => { const a = [...rs]; [a[sel], a[sel + 1]] = [a[sel + 1], a[sel]]; return a; }); setSel(sel + 1); }
    else if (k === "K" && cur && sel > 0) { setRules((rs) => { const a = [...rs]; [a[sel], a[sel - 1]] = [a[sel - 1], a[sel]]; return a; }); setSel(sel - 1); }
    else return;
    e.preventDefault();
  });

  const rg = cur ? ranges(SAMPLE, cur) : [];
  const segs: { t: string; hit: boolean }[] = [];
  let p = 0;
  rg.forEach(([a, b]) => { segs.push({ t: SAMPLE.slice(p, a), hit: false }, { t: SAMPLE.slice(a, b), hit: true }); p = b; });
  segs.push({ t: SAMPLE.slice(p), hit: false });

  return (
    <div className="fade-in flex h-full flex-col gap-4 pt-3">
      <Box
        active={focused}
        className="flex min-h-0 flex-1 flex-col"
        title={<><span className="font-semibold text-hi">设置</span><span className="text-dim">/ 净化规则</span></>}
        titleRight={`${rules.filter((r) => r.enabled).length}/${rules.length} 启用 · 按顺序执行`}
      >
        <div className="grid grid-cols-[2.2em_1.2fr_2.4fr_0.8fr_0.9fr] gap-3 border-b border-line px-2 pt-3 pb-1 pl-7 text-[12px] text-dim">
          <span>启用</span><span>名称</span><span>匹配</span><span>类型</span><span>作用域</span>
        </div>
        <div ref={ref} className="flex-1 overflow-y-auto py-1">
          {rules.map((r, i) => (
            <Row key={r.id} sel={i === sel} focused={focused} onClick={() => setSel(i)}>
              <div className="grid min-w-0 flex-1 grid-cols-[2.2em_1.2fr_2.4fr_0.8fr_0.9fr] items-center gap-3">
                <Toggle on={r.enabled} />
                <span className={r.enabled ? "truncate" : "truncate text-dim"}>{r.name}</span>
                <code className="truncate text-[12px] text-ok">{r.pattern}</code>
                <span className="text-[12px] text-mag">{r.isRegex ? "regex" : "文本"}</span>
                <span className="truncate text-[12px] text-info">{r.scope}</span>
              </div>
            </Row>
          ))}
        </div>
        <div className="border-t border-line px-3 py-1.5">
          <Hints items={[["space", "启用/禁用"], ["n", "新建"], ["d", "删除"], ["J/K", "调整顺序"]]} />
        </div>
      </Box>

      {cur && (
        <Box className="shrink-0 px-4 pt-4 pb-3" title={<span className="text-mute">预览 · {cur.name}</span>} titleRight={`命中 ${rg.length} 处`}>
          <div className="grid gap-x-3 gap-y-1 text-[13px] sm:grid-cols-[4em_1fr]">
            <span className="text-dim">替换为</span>
            <span className="text-fg">{cur.replacement ? <code className="text-ok">"{cur.replacement}"</code> : <span className="text-dim">（删除）</span>}</span>
            <span className="text-dim">原文</span>
            <span className="leading-relaxed">
              {segs.map((s, i) => (s.hit ? <span key={i} className="bg-err/15 text-err line-through">{s.t}</span> : <span key={i}>{s.t}</span>))}
            </span>
            <span className="text-dim">结果</span>
            <span className="leading-relaxed text-ok/90">
              {segs.map((s, i) => (s.hit ? (cur.replacement ? <span key={i} className="bg-ok/15">{cur.replacement}</span> : null) : <span key={i}>{s.t}</span>))}
            </span>
          </div>
        </Box>
      )}
    </div>
  );
}
