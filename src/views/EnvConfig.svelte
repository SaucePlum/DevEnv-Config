<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { portal } from "../lib/portal";
  import { store } from "../lib/store.svelte";
  import type { Params } from "../lib/types";
  import { t } from "../lib/i18n.svelte";
  import Toolbar from "../lib/components/Toolbar.svelte";
  import AppButton from "../lib/components/AppButton.svelte";
  import AppSelect from "../lib/components/AppSelect.svelte";
  import MirrorPicker from "../lib/components/MirrorPicker.svelte";
  import SshKeyPanel from "../lib/components/SshKeyPanel.svelte";
  import { askConfirm } from "../lib/confirm.svelte";

  let { onReload }: { onReload: () => void } = $props();
  let mirror: Record<string, string> = $state({});
  let form = $state<Params>({});
  let backups = $state<string[]>([]);
  let runningId = $state("");
  let configError = $state("");
  let sshKeys = $state<string[]>([]);
  let sshSelected = $state("");
  let activeTab = $state(0);
  let npmModal = $state(false);
  let npmLoading = $state(false);
  let identityModal = $state(false);
  let backupView = $state<{ name: string; content: string } | null>(null);

  const groups = [
    { title: "config.group_toolchain", ids: ["rust-mirror", "cargo-mirror"] },
    { title: "config.group_pkg", ids: ["npm-mirror", "pnpm-mirror", "yarn-mirror", "pip-mirror", "go-mirror", "nuget-mirror", "gem-mirror", "composer-mirror", "conda-mirror", "bun-mirror", "maven-mirror", "gradle-mirror", "cran-mirror"] },
    { title: "config.group_tools", ids: ["pnpm", "yarn"] },
    { title: "config.group_ai", ids: ["codex-cli", "claude-cli"] },
    { title: "config.group_dirs", ids: ["npm-dirs"] },
    { title: "config.group_identity", ids: ["git-identity", "ssh-key"] },
  ] as const;

  const keyName = (p: string) => p.split(/[\\/]/).pop() ?? p;

  async function loadSshKeys(prefer?: string) {
    sshKeys = await api.listSshPublicKeys().catch(() => []);
    if (prefer) {
      const hit = sshKeys.find((p) => keyName(p) === prefer);
      if (hit) { sshSelected = hit; return; }
    }
    if (!sshSelected || !sshKeys.includes(sshSelected)) sshSelected = sshKeys[0] ?? "";
  }

  onMount(async () => {
    const d = await api.configDefaults("git-identity").catch(() => ({} as Record<string, string>));
    form.git_name = d.git_name ?? "";
    form.git_email = d.git_email ?? "";
    backups = await api.listEnvBackups().catch(() => []);
    await loadSshKeys();
  });

  async function pickDir(key: "prefix_dir" | "cache_dir") {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const dir = await open({ directory: true, title: t("config.choose_dir_title") });
    if (typeof dir === "string") form[key] = dir;
  }

  async function configure(id: string, skipConfirm = false) {
    if (runningId) return;
    const name = store.items.find((i) => i.id === id)?.name ?? id;
    if (!skipConfirm) {
      const mirrorText = mirror[id] ? t("config.mirror_line", { url: mirror[id] }) : "";
      if (!(await askConfirm(t("config.run_confirm", { name, mirror: mirrorText })))) return;
    }
    runningId = id; configError = "";
    try { await api.runConfig(id, mirror[id] ?? null, form); }
    catch (e) { configError = String(e); }
    finally { runningId = ""; }
    if (id === "ssh-key") await loadSshKeys("id_ed25519.pub");
    onReload();
  }

  async function applyNpmDirs() {
    if (!form.prefix_dir || !form.cache_dir) { configError = t("config.pick_dirs_first"); return; }
    npmModal = false;
    await configure("npm-dirs", true);
  }

  // 打开弹窗时读取 npm 当前的 prefix / cache 回填，避免已配置却显示「未选择」
  async function openNpmModal() {
    npmModal = true;
    npmLoading = true;
    try {
      const d = await api.configDefaults("npm-dirs").catch(() => ({} as Record<string, string>));
      // 仅在尚未选择时回填，避免覆盖用户本次已选值
      if (!form.prefix_dir && d.prefix_dir) form.prefix_dir = d.prefix_dir;
      if (!form.cache_dir && d.cache_dir) form.cache_dir = d.cache_dir;
    } finally {
      npmLoading = false;
    }
  }

  async function applyIdentity() {
    if (!form.git_name?.trim() || !form.git_email?.trim()) { configError = t("config.fill_identity"); return; }
    identityModal = false;
    await configure("git-identity", true);
  }

  async function restore(b: string) {
    if (!(await askConfirm(t("config.restore_confirm", { b }), { danger: true }))) return;
    try { await api.restoreEnvBackup(b); onReload(); }
    catch (e) { configError = String(e); }
  }

  async function viewBackup(b: string) {
    const content = await api.readEnvBackup(b).catch((e) => String(e));
    backupView = { name: b, content };
  }

  async function delBackup(b: string) {
    if (!(await askConfirm(t("config.delete_backup_confirm", { b }), { danger: true }))) return;
    await api.deleteEnvBackup(b).catch((e) => { configError = String(e); });
    backups = await api.listEnvBackups().catch(() => []);
  }
</script>

<Toolbar
  title={t("config.title")}
  showLts={false}
  stats={[
    { label: t("toolbar.configured"), count: store.items.filter((i) => i.kind === "config" && i.status === "configured").length },
    { label: t("toolbar.not_configured"), count: store.items.filter((i) => i.kind === "config" && i.status !== "configured").length },
  ]}
/>

<div class="group" style="padding-bottom:0">
  <div class="tabs">
    {#each groups as g, i (g.title)}
      <button class="tab" class:on={activeTab === i} onclick={() => (activeTab = i)}>{t(g.title)}</button>
    {/each}
  </div>
</div>

{#if configError}
  <div class="group" style="padding-top:0"><div style="color:var(--err);font-size:12.5px">{t("config.op_failed", { err: configError })}</div></div>
{/if}

{#if groups[activeTab]}
  {@const g = groups[activeTab]}
  <div class="group">
    <div class="group-head"><h2>{t(g.title)}</h2><span class="badge mono">{g.ids.length}</span></div>
    <div class="card">
      {#each g.ids as id (id)}
        {@const it = store.items.find((i) => i.id === id)}
        {#if it}
          <div class="row" data-st={it.status === "configured" ? "done" : ""}>
            <div class="icon-box" class:done={it.status === "configured"}>{it.name.slice(0, 1)}</div>
            <div>
              <div class="name-line">
                <span class="nm">{it.name}</span>
                <span class="pill" class:ok={it.status === "configured"}>
                  {it.status === "configured" ? t("toolbar.configured") : t("toolbar.not_configured")}
                </span>
              </div>
            </div>
            <div>
              {#if id === "git-identity"}
                <span class="mono" style="color:var(--text-3)">{t("config.fill_in_modal")}</span>
              {:else if id === "npm-dirs"}
                <span class="mono" style="color:var(--text-3)">{t("config.choose_in_modal")}</span>
              {:else if id === "ssh-key"}
                {#if sshKeys.length}
                  <AppSelect
                    options={sshKeys.map((k) => ({ value: k, label: keyName(k) }))}
                    bind:value={sshSelected}
                    width="100%"
                  />
                {:else}
                  <span class="mono" style="color:var(--text-3)">{t("config.no_pub")}</span>
                {/if}
              {:else if it.mirrors.length > 0}
                <MirrorPicker mirrors={it.mirrors} current={it.mirror_current} bind:value={mirror[id]} />
              {:else}
                <span class="mono" style="color:var(--text-3)">—</span>
              {/if}
            </div>
            <div class="mono" style="color:var(--text-3);word-break:break-all">
              {#if id === "ssh-key"}{sshSelected || "—"}{:else}{it.detail ?? "—"}{/if}
            </div>
            <div style="display:flex;gap:8px;justify-content:flex-end">
              {#if id === "ssh-key"}
                <SshKeyPanel onRun={() => configure(id, true)} keys={sshKeys} bind:selected={sshSelected} />
              {:else if id === "npm-dirs"}
                <AppButton size="sm" onclick={openNpmModal}>{t("config.set")}</AppButton>
              {:else if id === "git-identity"}
                <AppButton size="sm" disabled={runningId === id} onclick={() => (identityModal = true)}>{t("config.configure")}</AppButton>
              {:else}
                <AppButton size="sm" disabled={runningId === id} onclick={() => configure(id)}>{t("config.configure")}</AppButton>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<div class="group">
  <div class="group-head"><h2>{t("config.backups")}</h2><span class="badge mono">{backups.length}</span></div>
  <div class="card">
    {#each backups as b (b)}
      <div class="row" style="grid-template-columns:1fr auto">
        <span class="mono" style="word-break:break-all">{b}</span>
        <div style="display:flex;gap:8px">
          <AppButton size="sm" onclick={() => viewBackup(b)}>{t("config.view")}</AppButton>
          <AppButton size="sm" onclick={() => restore(b)}>{t("config.restore")}</AppButton>
          <AppButton size="sm" variant="danger" onclick={() => delBackup(b)}>{t("common.delete")}</AppButton>
        </div>
      </div>
    {/each}
    {#if !backups.length}<div class="row" style="grid-template-columns:1fr"><span style="color:var(--text-3)">{t("config.no_backups")}</span></div>{/if}
  </div>
</div>

{#if backupView}
  {@const bv = backupView}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>{t("config.backup_content")}</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (backupView = null)} title={t("common.close")}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="mono" style="word-break:break-all;color:var(--text-3);margin-bottom:8px">{bv.name}</div>
        <textarea class="field" style="height:280px;width:100%" readonly bind:value={bv.content}></textarea>
        <div style="font-size:12px;color:var(--text-3);margin-top:6px">
          {t("config.backup_hint")}
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={async () => await api.copyText(bv.content)}>{t("config.copy_content")}</AppButton>
        <AppButton onclick={() => (backupView = null)}>{t("common.close")}</AppButton>
      </div>
    </div>
  </div>
{/if}

{#if npmModal}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>{t("config.npm_title")}</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (npmModal = false)} title={t("common.close")}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="display:flex;flex-direction:column;gap:14px">
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">{t("config.npm_prefix_label")}</div>
            <div style="display:flex;gap:8px;align-items:center">
              <span class="mono" style="flex:1;color:var(--text-2);word-break:break-all" title={form.prefix_dir}>{npmLoading ? t("config.reading") : (form.prefix_dir || t("config.not_selected"))}</span>
              <AppButton size="sm" onclick={() => pickDir("prefix_dir")}>{t("config.choose")}</AppButton>
            </div>
          </div>
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">{t("config.npm_cache_label")}</div>
            <div style="display:flex;gap:8px;align-items:center">
              <span class="mono" style="flex:1;color:var(--text-2);word-break:break-all" title={form.cache_dir}>{npmLoading ? t("config.reading") : (form.cache_dir || t("config.not_selected"))}</span>
              <AppButton size="sm" onclick={() => pickDir("cache_dir")}>{t("config.choose")}</AppButton>
            </div>
          </div>
          <div style="font-size:12px;color:var(--text-3)">
            {t("config.npm_hint")}
          </div>
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={() => (npmModal = false)}>{t("common.cancel")}</AppButton>
        <AppButton variant="primary" onclick={applyNpmDirs} disabled={runningId === "npm-dirs"}>{t("config.save_and_apply")}</AppButton>
      </div>
    </div>
  </div>
{/if}

{#if identityModal}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>{t("config.identity_title")}</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (identityModal = false)} title={t("common.close")}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="display:flex;flex-direction:column;gap:14px">
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">{t("config.identity_name_label")}</div>
            <input class="field" style="width:100%" placeholder={t("config.identity_name_ph")} bind:value={form.git_name} />
          </div>
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">{t("config.identity_email_label")}</div>
            <input class="field" style="width:100%" placeholder={t("config.identity_email_ph")} bind:value={form.git_email} />
          </div>
          <div style="font-size:12px;color:var(--text-3)">
            {t("config.identity_hint")}
          </div>
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={() => (identityModal = false)}>{t("common.cancel")}</AppButton>
        <AppButton variant="primary" onclick={applyIdentity} disabled={runningId === "git-identity"}>{t("config.save_and_apply")}</AppButton>
      </div>
    </div>
  </div>
{/if}
