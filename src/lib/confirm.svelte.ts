// 全局二次确认 / 提示弹窗服务。
// askConfirm 返回 Promise<boolean>（确定 = true，取消 = false）；
// askAlert 仅展示信息，返回 Promise<void>。由 App 根部挂载的 ConfirmModal 渲染。

import { t } from "./i18n.svelte";

type Mode = "confirm" | "alert";

const state = $state({
  open: false,
  title: "",
  message: "",
  mode: "confirm" as Mode,
  danger: false,
});

let resolver: ((v: boolean) => void) | null = null;

export const confirmState = state;

export function askConfirm(
  message: string,
  options?: { title?: string; danger?: boolean }
): Promise<boolean> {
  state.title = options?.title ?? t("confirm.title");
  state.message = message;
  state.mode = "confirm";
  state.danger = options?.danger ?? false;
  state.open = true;
  return new Promise((resolve) => {
    resolver = resolve;
  });
}

export function askAlert(message: string, title = ""): Promise<void> {
  state.title = title || t("alert.title");
  state.message = message;
  state.mode = "alert";
  state.danger = false;
  state.open = true;
  return new Promise((resolve) => {
    resolver = () => resolve();
  });
}

export function answerConfirm(v: boolean) {
  state.open = false;
  const r = resolver;
  resolver = null;
  r?.(v);
}
