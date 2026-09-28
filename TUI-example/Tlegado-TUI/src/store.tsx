import { createContext, useContext } from "react";
import type { Book, BookSource, HistoryItem, Prefs, PurifyRule } from "@/data/mock";

export type Route =
  | { kind: "home" }
  | { kind: "shelf"; filter: "all" | "local" | "network" }
  | { kind: "search" }
  | { kind: "discover"; sourceId: string }
  | { kind: "history" }
  | { kind: "sources" }
  | { kind: "purify" }
  | { kind: "prefs" };

export interface ReaderState {
  book: Book;
  chapter: number;
}

export interface AppCtx {
  books: Book[];
  setBooks: React.Dispatch<React.SetStateAction<Book[]>>;
  sources: BookSource[];
  setSources: React.Dispatch<React.SetStateAction<BookSource[]>>;
  rules: PurifyRule[];
  setRules: React.Dispatch<React.SetStateAction<PurifyRule[]>>;
  prefs: Prefs;
  setPrefs: React.Dispatch<React.SetStateAction<Prefs>>;
  history: HistoryItem[];
  setHistory: React.Dispatch<React.SetStateAction<HistoryItem[]>>;
  openReader: (book: Book, chapter?: number) => void;
  addToShelf: (book: Book) => void;
  toast: (msg: string, tone?: "ok" | "err" | "info") => void;
  navigate: (r: Route) => void;
  showHelp: () => void;
}

export const Ctx = createContext<AppCtx>(null as unknown as AppCtx);
export const useApp = () => useContext(Ctx);
