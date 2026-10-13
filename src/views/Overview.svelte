<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { ItemView, Params } from "../lib/types";
  import type { MsgKey } from "../lib/i18n/messages";
  import { t } from "../lib/i18n.svelte";
  import Toolbar from "../lib/components/Toolbar.svelte";
  import ItemRow from "../lib/components/ItemRow.svelte";
  import ProgressModal from "../lib/components/ProgressModal.svelte";
  import { askAlert, askConfirm } from "../lib/confirm.svelte";

  let { onReload }: { onReload: () => void } = $props();
  let chosen: Record<string, string> = $state({});
  let checked: Record<string, boolean> = $state({});
  let modal = $state<{ id: string; name: string } | null>(null);
  let installingAll = $state(false);

  const order = ["runtime", "build", "database", "web", "vcredist", "ide"];
  const categoryLabel = (k: string) => t(`category.${k}` as MsgKey);

  // 分类选项卡：全部 + 各分类
  let tab = $state("all");
  const tabs = $derived([
    { key: "all", label: t("overview.all") },
    ...order
      .filter((k) => store.items.some((i) => i.category === k))
      .map((k) => ({ key: k, label: categoryLabel(k) })),
  ]);
  const visible = $derived(tab === "all" ? store.items : store.items.filter((i) => i.category === tab));
  const groups = $derived(order.filter((k) => visible.some((i) => i.category === k)));

  let fetchingVersions = $state(false);

  // 并发拉取各软件的版本列表；失败的记入 versionError，「重新检测」会重试它们
  async function loadVersions(items: ItemView[]) {
    fetchingVersions = true;
    try {
      await Promise.allSettled(
        items.map(async (it) => {
          if (it.kind === "config") return;
          if (it.source === "local" && !it.archive) return;
          try {
            store.versions[it.id] = await api.listVersions(it.id);
          } catch (e) {
            store.versionError[it.id] = String(e);
          }
        })
      );
    } finally {
      fetchingVersions = false;
    }
  }

  $effect(() => {
    const pending = store.items.filter(
      (i) =>
        i.kind !== "config" &&
        !(i.source === "local" && !i.archive) &&
        !store.versions[i.id] &&
        !store.versionError[i.id]
    );
    if (pending.length && !fetchingVersions) loadVersions(pending);
  });

  async function install(it: ItemView, extra: Params = {}) {
    const params: Params = { ...extra };
    // 二次确认：重装给出更明确的提示
    const verb = it.status === "installed" ? t("item.reinstall") : t("item.install");
    const ver = chosen[it.id] ? t("overview.version_suffix", { v: chosen[it.id] }) : "";
    const hint = it.status === "installed" ? t("overview.reinstall_hint") : "";
    if (!(await askConfirm(t("overview.confirm", { verb, name: it.name, ver, hint }), { danger: it.status === "installed" }))) return;
    if (it.needs_dir) {
      const dir = await open({ directory: true, title: t("overview.choose_dir", { name: it.name }) });
      if (!dir || typeof dir !== "string") return;
      params.install_dir = dir;
      if (await api.dirNeedsConfirm(dir)) {
        if (!(await askConfirm(t("overview.dir_exists", { dir }), { danger: true }))) return;
      }
    }
    modal = { id: it.id, name: it.name };
    try {
      await api.startItem(it.id, chosen[it.id] || null, params);
    } catch (e) {
      store.live[it.id] = { phase: "failed", message: String(e) };
    }
  }

  async function installChecked() {
    if (installingAll) return;
    installingAll = true;
    const failed: string[] = [];
    try {
      for (const it of store.items) {
        if (!checked[it.id]) continue;
        try { await install(it); } catch (e) { failed.push(`${it.name}: ${String(e)}`); }
      }
    } finally { installingAll = false; }
    if (failed.length) await askAlert(t("overview.batch_failed", { list: failed.join("\n") }), t("overview.batch_title"));
    onReload();
  }
</script>

<Toolbar title={t("overview.title")} onInstallAll={installChecked} onRefresh={onReload} installDisabled={installingAll} />

<div class="group" style="padding-bottom:0">
  <div class="tabs">
    {#each tabs as tb (tb.key)}
      <button class="tab" class:on={tab === tb.key} onclick={() => (tab = tb.key)}>{tb.label}</button>
    {/each}
  </div>
</div>

{#each groups as g (g)}
  <div class="group">
    <div class="group-head">
      <h2>{categoryLabel(g)}</h2>
      <span class="badge mono">{visible.filter((i) => i.category === g).length}</span>
      <span class="grow"></span>
    </div>
    <div class="card">
      {#each visible.filter((i) => i.category === g) as it (it.id)}
        <ItemRow
          item={it}
          versions={(store.versions[it.id] ?? []).filter((v) => !store.ltsOnly || v.lts || it.source === "local")}
          bind:version={chosen[it.id]}
          selecting={!!store.versions[it.id]}
          bind:checked={checked[it.id]}
          onInstall={(p) => install(it, p)}
        />
      {/each}
    </div>
  </div>
{/each}

{#if modal}
  <ProgressModal id={modal.id} name={modal.name} onClose={() => { modal = null; onReload(); }} />
{/if}
