import { cn } from "@/utils/cn";
import { Box, useScrollIntoView } from "./tui";
import type { Route } from "@/store";
import type { ReactNode } from "react";

export interface NavItem {
  id: string;
  section: string | null;
  label: string;
  route: Route;
  right?: ReactNode;
  disabled?: boolean;
}

export function Sidebar({
  items,
  index,
  focused,
  onSelect,
  collapsed,
}: {
  items: NavItem[];
  index: number;
  focused: boolean;
  onSelect: (i: number) => void;
  collapsed: Record<string, boolean>;
}) {
  const ref = useScrollIntoView(index);
  let lastSection: string | null = "__";

  return (
    <Box
      active={focused}
      className="flex h-full flex-col pt-3"
      title={
        <>
          <span className={focused ? "text-accent" : "text-mute"}>●</span>
          <span className="font-semibold text-hi">Tlegado</span>
        </>
      }
      titleRight="tab ⇄"
    >
      <div ref={ref} className="no-scrollbar flex-1 overflow-y-auto pb-2">
        {items.map((it, i) => {
          const header = it.section !== lastSection && it.section;
          lastSection = it.section;
          const hidden = it.section && collapsed[it.section];
          const sel = i === index;
          return (
            <div key={it.id}>
              {header && (
                <div className="mt-2 flex items-center gap-1 px-3 text-[12px] tracking-wider text-dim">
                  <span>{collapsed[it.section!] ? "▸" : "▾"}</span>
                  <span>{it.section}</span>
                  <span className="ml-1 flex-1 overflow-hidden whitespace-nowrap text-line">
                    {"─".repeat(40)}
                  </span>
                </div>
              )}
              {!hidden && (
                <div
                  data-sel={sel}
                  onClick={() => onSelect(i)}
                  className={cn(
                    "flex cursor-pointer items-center gap-1.5 px-2 select-none",
                    it.section ? "pl-4" : "pl-2",
                    sel && focused && "bg-bg3 text-hi",
                    sel && !focused && "text-hi",
                    !sel && "hover:bg-bg2",
                    it.disabled && "opacity-50",
                  )}
                >
                  <span className={cn("w-3", sel ? (focused ? "text-accent" : "text-mute") : "text-transparent")}>
                    ›
                  </span>
                  <span className="flex-1 truncate">{it.label}</span>
                  {it.right && <span className="shrink-0 text-[12px] text-dim">{it.right}</span>}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </Box>
  );
}
