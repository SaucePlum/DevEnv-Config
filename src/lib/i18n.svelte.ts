// 界面语言状态与文案查询。
// - `zh` / `en` 词典见 ./i18n/messages.ts
// - 组件内直接调用 `t("key")` 即可，切换语言时模板会自动重新渲染
// - 语言选择写入 localStorage，下次启动沿用；同时通知后端，使软件名/备注/进度/日志/错误随之切换

import { api } from "./api";
import { zh, en, type MsgKey } from "./i18n/messages";

export type Lang = "zh" | "en";
const LS_KEY = "devenv.lang";

export const i18n = $state({ lang: "zh" as Lang });

/** 应用语言：更新状态、持久化、通知后端。返回后端写入完成的 Promise。 */
export function applyLang(l: Lang): Promise<void> {
  i18n.lang = l;
  try {
    localStorage.setItem(LS_KEY, l);
  } catch {
    /* 存储不可用时忽略 */
  }
  document.documentElement.setAttribute("lang", l === "zh" ? "zh-CN" : "en");
  return api.setLanguage(l).catch(() => {});
}

/** 启动时初始化语言：读取本地存储，默认中文。应在首次拉取数据前 await。 */
export function initLang(): Promise<void> {
  let stored: Lang | null = null;
  try {
    const v = localStorage.getItem(LS_KEY);
    if (v === "zh" || v === "en") stored = v;
  } catch {
    /* 存储不可用时用默认值 */
  }
  return applyLang(stored ?? "zh");
}

/** 用户主动切换语言：改语言后通知界面重新拉取后端数据（软件名、备注、检测详情）。 */
export function switchLang(l: Lang) {
  applyLang(l).then(() => window.dispatchEvent(new Event("devenv:lang")));
}

/** 取当前语言下的文案，支持 `{name}` 形式的占位符替换。 */
export function t(key: MsgKey, params?: Record<string, string | number>): string {
  const table: Record<MsgKey, string> = i18n.lang === "en" ? en : zh;
  let s = table[key] ?? zh[key] ?? String(key);
  if (params) {
    for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
  }
  return s;
}
