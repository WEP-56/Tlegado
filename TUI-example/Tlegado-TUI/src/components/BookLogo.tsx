import { useEffect, useRef, useState } from "react";
import { PALETTE, renderFrame, type Frame } from "@/anim/bookLogo";

/** 盲文字形优先使用含 Braille 的等宽字体，保证每格等宽 */
const BRAILLE_FONT =
  '"Cascadia Mono", "DejaVu Sans Mono", "Noto Sans Symbols 2", "Apple Braille", "Segoe UI Symbol", monospace';

/** 与终端一致的刷新率 */
const FPS = 30;

/**
 * 首页点阵书本动画：纯装饰、持续循环自动播放，无任何交互。
 * 时间轴：散点汇聚(1.8s) → 读左页 → 读右页 → 翻页 → 停顿 → 无限循环。
 */
export function BookLogo({ className, pageBase = 0 }: { className?: string; pageBase?: number }) {
  const [frame, setFrame] = useState<Frame>(() => renderFrame(0));
  const start = useRef(0);

  useEffect(() => {
    start.current = performance.now();
    let raf = 0;
    let last = -1;
    const loop = (now: number) => {
      const t = now - start.current;
      const f = Math.floor(t / (1000 / FPS));
      if (f !== last) {
        last = f;
        setFrame(renderFrame(t));
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    // 标签页切回时 rAF 会暂停，恢复时把起点前移避免时间跳变
    const onVis = () => {
      if (!document.hidden) start.current = performance.now();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      cancelAnimationFrame(raf);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  const p = pageBase + frame.spread * 2 + 1;

  return (
    <div aria-hidden className={className}>
      <div className="whitespace-pre select-none" style={{ fontFamily: BRAILLE_FONT, fontSize: 13, lineHeight: 1.22 }}>
        {frame.rows.map((row, i) => (
          <div key={i}>
            {row.map((s, j) => (
              <span key={j} style={{ color: PALETTE[s.layer] }}>{s.text}</span>
            ))}
          </div>
        ))}
      </div>
      <div className="mt-1 text-center text-[11px] leading-none text-dim select-none">
        {p} <span className="text-line">·</span> {p + 1}
      </div>
    </div>
  );
}
