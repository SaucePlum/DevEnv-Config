<script lang="ts">
  import type { ItemView, Params, VersionInfo } from "../types";
  import { store } from "../store.svelte";
  import { t } from "../i18n.svelte";
  import StatusBadge from "./StatusBadge.svelte";
  import VersionPicker from "./VersionPicker.svelte";
  import AppButton from "./AppButton.svelte";

  let {
    item, versions = [], version = $bindable(), selecting = false, checked = $bindable(),
    onInstall, onConfigure,
  }: {
    item: ItemView;
    versions?: VersionInfo[];
    version?: string;
    selecting?: boolean;
    checked?: boolean;
    onInstall?: (p: Params) => void;
    onConfigure?: () => void;
  } = $props();

  const live = $derived(store.live[item.id]);
  const busy = $derived(!!live && ["downloading", "verifying_hash", "waiting_user", "wizard_running", "extracting", "moving", "finalizing", "verifying"].includes(live.phase));
  const blocked = $derived(item.requires.length > 0 && item.requires.some((r) => store.items.find((i) => i.id === r)?.status !== "installed"));
  // 依赖项用当前语言的显示名（拿不到时回退 id）
  const requiresText = $derived(item.requires.map((r) => store.items.find((i) => i.id === r)?.name ?? r).join(t("item.requires_sep")));
</script>

<div class="row" data-st={item.status === "installed" || item.status === "configured" ? "done" : live?.phase === "failed" ? "fail" : busy ? "busy" : ""}>
  <div class="icon-box" class:done={item.status === "installed" || item.status === "configured"} class:busy={busy}>
    {item.name.slice(0, 1)}
  </div>

  <div>
    <div class="name-line">
      <span class="nm" class:busy={busy}>{item.name}</span>
      <StatusBadge status={item.status} phase={live?.phase} message={live?.message ?? ""} />
      {#if item.source === "local"}<span class="chip">{t("item.bundled")}</span>{/if}
      {#if item.archive}<span class="chip mono">{item.archive}</span>{/if}
    </div>
    {#if live}
      <div style="color:var(--text-3);font-size:12px">{live.message}</div>
    {/if}
    {#if blocked}
      <div style="color:var(--warn);font-size:12px">{t("item.requires", { list: requiresText })}</div>
    {/if}
  </div>

  <div>
    {#if item.kind === "config"}
      <span class="mono" style="color:var(--text-3)">{item.mirrors.length ? t("item.mirror_count", { n: item.mirrors.length }) : "—"}</span>
    {:else if item.source === "local" && !versions.length}
      <span class="mono" style="color:var(--text-3)">{t("item.bundled_resource")}</span>
    {:else if versions.length || selecting}
      <VersionPicker {versions} bind:value={version} enabled={!busy && !blocked} />
    {:else}
      <span class="mono" style="color:var(--text-3)">{t("item.fixed_version")}</span>
    {/if}
  </div>

  <div class="mono" style="color:var(--text-3)">{item.version ? t("item.installed_version", { v: item.version }) : "—"}</div>

  <div style="display:flex;gap:8px;justify-content:flex-end">
    <input type="checkbox" bind:checked disabled={busy || item.kind === "config"} />
    {#if item.kind === "config"}
      <AppButton size="sm" disabled={busy || blocked} onclick={() => onConfigure?.()}>{t("item.configure")}</AppButton>
    {:else}
      <AppButton size="sm" disabled={busy || blocked} onclick={() => onInstall?.({})}>
        {item.status === "installed" ? t("item.reinstall") : t("item.install")}
      </AppButton>
    {/if}
  </div>
</div>
