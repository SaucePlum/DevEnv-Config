import type { ItemEvent, ItemView, LogEntry, Phase, VersionInfo } from "./types";

export interface Live { phase: Phase; message: string; progress?: number; attempted?: string[] }

export const store = $state({
  items: [] as ItemView[],
  versions: {} as Record<string, VersionInfo[]>,
  versionError: {} as Record<string, string>,
  live: {} as Record<string, Live>,
  logs: [] as LogEntry[],
  page: "overview" as "overview" | "config" | "download" | "about",
  theme: "light" as "light" | "dark",
  ltsOnly: false,
  elevated: true,
  cacheMb: 0,
});

export function applyItemEvent(e: ItemEvent) {
  store.live[e.id] = { phase: e.phase, message: e.message, progress: e.progress, attempted: e.attempted };
  if (e.phase === "done") {
    const it = store.items.find((i) => i.id === e.id);
    if (it) it.status = "installed";
  }
  if (e.phase === "configured") {
    const it = store.items.find((i) => i.id === e.id);
    if (it) it.status = "configured";
  }
}

let logSeq = 0;

export function pushLog(e: LogEntry) {
  store.logs = [...store.logs.slice(-499), { ...e, seq: ++logSeq }];
}

export function setTheme(t: "light" | "dark") {
  store.theme = t;
  document.documentElement.setAttribute("data-theme", t);
}
