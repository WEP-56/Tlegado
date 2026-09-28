import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/utils/cn";
import { SPINNER } from "@/data/mock";

/** 边框标题嵌入式面板（参考 Grok Build / opencode） */
export function Box({
  title,
  titleRight,
  footer,
  active,
  className,
  children,
  onClick,
}: {
  title?: ReactNode;
  titleRight?: ReactNode;
  footer?: ReactNode;
  active?: boolean;
  className?: string;
  children?: ReactNode;
  onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      className={cn(
        "relative rounded-[7px] border transition-colors",
        active ? "border-line2" : "border-line",
        className,
      )}
    >
      {title && (
        <div className="pointer-events-none absolute -top-[0.8em] left-3 z-10 flex items-center gap-1 bg-bg px-1.5 text-[13px] leading-[1.5]">
          {title}
        </div>
      )}
      {titleRight && (
        <div className="absolute -top-[0.8em] right-3 z-10 bg-bg px-1.5 text-[12px] leading-[1.5] text-dim">
          {titleRight}
        </div>
      )}
      {footer && (
        <div className="absolute -bottom-[0.8em] right-3 z-10 bg-bg px-1.5 text-[12px] leading-[1.5] text-mute">
          {footer}
        </div>
      )}
      {children}
    </div>
  );
}

/** ━━━━───── 进度条 */
export function Bar({ value, width = 12, color = "text-accent" }: { value: number; width?: number; color?: string }) {
  const v = Math.max(0, Math.min(1, value));
  const n = Math.round(v * width);
  return (
    <span className="whitespace-pre tracking-[-0.05em]">
      <span className={color}>{"━".repeat(n)}</span>
      <span className="text-line">{"━".repeat(width - n)}</span>
    </span>
  );
}

export function Kbd({ k, label }: { k: string; label?: string }) {
  return (
    <span className="whitespace-nowrap">
      <span className="text-fg">{k}</span>
      {label && <span className="text-dim"> {label}</span>}
    </span>
  );
}

export function Hints({ items }: { items: [string, string][] }) {
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[12px]">
      {items.map(([k, l], i) => (
        <span key={i} className="whitespace-nowrap">
          <span className="text-mute">{k}</span> <span className="text-dim">{l}</span>
        </span>
      ))}
    </div>
  );
}

/** 视图级键盘监听，仅在 active 时生效；输入框聚焦时不触发 */
export function useKeys(active: boolean, handler: (e: KeyboardEvent) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    if (!active) return;
    const fn = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      ref.current(e);
    };
    window.addEventListener("keydown", fn);
    return () => window.removeEventListener("keydown", fn);
  }, [active]);
}

export function useSpinner(on = true, ms = 80) {
  const [i, setI] = useState(0);
  useEffect(() => {
    if (!on) return;
    const t = setInterval(() => setI((x) => (x + 1) % SPINNER.length), ms);
    return () => clearInterval(t);
  }, [on, ms]);
  return SPINNER[i];
}

/** 让选中行保持在可视区 */
export function useScrollIntoView(dep: unknown) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>("[data-sel='true']");
    el?.scrollIntoView({ block: "nearest" });
  }, [dep]);
  return ref;
}

/** 列表行：选中时左侧 › 光标 + 背景高亮 */
export function Row({
  sel,
  focused,
  children,
  className,
  onClick,
  onDoubleClick,
}: {
  sel: boolean;
  focused: boolean;
  children: ReactNode;
  className?: string;
  onClick?: () => void;
  onDoubleClick?: () => void;
}) {
  return (
    <div
      data-sel={sel}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      className={cn(
        "flex cursor-pointer items-center gap-2 px-2 select-none",
        sel && focused && "bg-bg3 text-hi",
        sel && !focused && "bg-bg2",
        !sel && "hover:bg-bg2",
        className,
      )}
    >
      <span className={cn("w-3 shrink-0", sel ? (focused ? "text-accent" : "text-dim") : "text-transparent")}>›</span>
      {children}
    </div>
  );
}

/** 标签切换：[ 玄幻 ]  奇幻  仙侠 */
export function Tabs({
  items,
  index,
  onChange,
  focused,
}: {
  items: string[];
  index: number;
  onChange: (i: number) => void;
  focused?: boolean;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-1 text-[13px]">
      {items.map((t, i) => (
        <button
          key={t}
          onClick={() => onChange(i)}
          className={cn(
            "cursor-pointer px-1",
            i === index ? (focused ? "text-accent" : "text-hi") : "text-dim hover:text-fg",
          )}
        >
          {i === index ? `[${t}]` : ` ${t} `}
        </button>
      ))}
    </div>
  );
}

export function Toggle({ on }: { on: boolean }) {
  return <span className={on ? "text-ok" : "text-dim"}>{on ? "[✓]" : "[ ]"}</span>;
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="px-4 py-6 text-dim">{children}</div>;
}
