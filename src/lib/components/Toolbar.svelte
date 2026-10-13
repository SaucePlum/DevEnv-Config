<script lang="ts">
  import { store } from "../store.svelte";
  import { t } from "../i18n.svelte";
  import AppButton from "./AppButton.svelte";

  let {
    title,
    onInstallAll,
    onRefresh,
    installDisabled = false,
    showLts = true,
    stats,
  }: {
    title: string;
    onInstallAll?: () => void;
    onRefresh?: () => void;
    installDisabled?: boolean;
    /** 是否显示「仅看 LTS」开关（软件页用） */
    showLts?: boolean;
    /** 自定义统计项；不传时默认显示 已装 / 未装 */
    stats?: { label: string; count: number }[];
  } = $props();

  const installed = $derived(store.items.filter((i) => i.status === "installed").length);
  const pending = $derived(store.items.filter((i) => i.status === "not_installed").length);
  const statItems = $derived(stats ?? [
    { label: t("toolbar.installed"), count: installed },
    { label: t("toolbar.not_installed"), count: pending },
  ]);
</script>

<div class="toolbar">
  <h1>{title}</h1>
  <div class="stats">
    {#each statItems as s (s.label)}
      <span class="stat">{s.label} <b class="mono">{s.count}</b></span>
    {/each}
  </div>
  <div class="tb-actions">
    {#if showLts}
      <button class="switch" class:on={store.ltsOnly} onclick={() => (store.ltsOnly = !store.ltsOnly)}>
        <span class="tk"></span>{t("toolbar.lts_only")}
      </button>
    {/if}
    {#if onRefresh}
      <AppButton size="sm" onclick={onRefresh}>{t("toolbar.redetect")}</AppButton>
    {/if}
    {#if onInstallAll}
      <AppButton size="sm" variant="primary" onclick={onInstallAll} disabled={installDisabled}>{t("toolbar.install_selected")}</AppButton>
    {/if}
  </div>
</div>
