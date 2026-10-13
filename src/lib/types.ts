export type DetectStatus = "installed" | "not_installed" | "configured" | "not_configured";

export type Phase =
  | "not_installed" | "installed" | "drift"
  | "downloading" | "verifying_hash" | "waiting_user" | "wizard_running"
  | "extracting" | "moving" | "finalizing" | "verifying"
  | "done" | "failed" | "not_configured" | "configured";

export interface Mirror { name: string; url: string }

export interface ItemView {
  id: string;
  name: string;
  category: string;
  source: "online" | "local" | "config";
  kind: "installer" | "portable" | "config";
  note?: string;
  requires: string[];
  mirrors: Mirror[];
  status: DetectStatus;
  version?: string;
  detail?: string;
  mirror_current?: string;
  needs_dir: boolean;
  has_verify: boolean;
  archive?: string;
}

export interface VersionInfo {
  version: string;
  lts: boolean;
  lts_name?: string;
  date?: string;
  recommended: boolean;
}

export interface ItemEvent {
  id: string;
  phase: Phase;
  message: string;
  progress?: number;
  attempted?: string[];
  exit_code?: number;
}

export interface LogEntry { ts: string; level: string; item?: string; message: string; seq?: number }

export interface StepLog { ok: boolean; message: string }

export interface Params {
  install_dir?: string;
  git_name?: string;
  git_email?: string;
  prefix_dir?: string;
  cache_dir?: string;
}

export interface RuntimeState {
  phase: Phase;
  message: string;
  progress?: number;
  exitCode?: number;
  attempted?: string[];
}
