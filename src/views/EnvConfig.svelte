<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { portal } from "../lib/portal";
  import { store } from "../lib/store.svelte";
  import type { Params } from "../lib/types";
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
    { title: "工具链镜像", ids: ["rust-mirror", "cargo-mirror"] },
    { title: "包管理器换源", ids: ["npm-mirror", "pnpm-mirror", "yarn-mirror", "pip-mirror", "go-mirror", "nuget-mirror", "gem-mirror", "composer-mirror", "conda-mirror", "bun-mirror", "maven-mirror", "gradle-mirror", "cran-mirror"] },
    { title: "工具安装", ids: ["pnpm", "yarn"] },
    { title: "AI 编程 CLI", ids: ["codex-cli", "claude-cli"] },
    { title: "目录设置", ids: ["npm-dirs"] },
    { title: "身份与密钥", ids: ["git-identity", "ssh-key"] },
  ];

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
    const dir = await open({ directory: true, title: "选择目录" });
    if (typeof dir === "string") form[key] = dir;
  }

  async function configure(id: string, skipConfirm = false) {
    if (runningId) return;
    const name = store.items.find((i) => i.id === id)?.name ?? id;
    if (!skipConfirm) {
      const mirrorText = mirror[id] ? `\n镜像：${mirror[id]}` : "";
      if (!(await askConfirm(`确认执行「${name}」？${mirrorText}\n执行前会自动备份相关配置。`))) return;
    }
    runningId = id; configError = "";
    try { await api.runConfig(id, mirror[id] ?? null, form); }
    catch (e) { configError = String(e); }
    finally { runningId = ""; }
    if (id === "ssh-key") await loadSshKeys("id_ed25519.pub");
    onReload();
  }

  async function applyNpmDirs() {
    if (!form.prefix_dir || !form.cache_dir) { configError = "请先选择全局包目录与缓存目录。"; return; }
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
    if (!form.git_name?.trim() || !form.git_email?.trim()) { configError = "请填写用户名与邮箱。"; return; }
    identityModal = false;
    await configure("git-identity", true);
  }

  async function restore(b: string) {
    if (!(await askConfirm(`确定还原 ${b} 吗？将覆盖当前用户级与系统级环境变量。`, { danger: true }))) return;
    try { await api.restoreEnvBackup(b); onReload(); }
    catch (e) { configError = String(e); }
  }

  async function viewBackup(b: string) {
    const content = await api.readEnvBackup(b).catch((e) => String(e));
    backupView = { name: b, content };
  }

  async function delBackup(b: string) {
    if (!(await askConfirm(`删除备份 ${b}？删除后无法恢复。`, { danger: true }))) return;
    await api.deleteEnvBackup(b).catch((e) => { configError = String(e); });
    backups = await api.listEnvBackups().catch(() => []);
  }
</script>

<Toolbar
  title="环境变量配置"
  showLts={false}
  stats={[
    { label: "已配置", count: store.items.filter((i) => i.kind === "config" && i.status === "configured").length },
    { label: "未配置", count: store.items.filter((i) => i.kind === "config" && i.status !== "configured").length },
  ]}
/>

<div class="group" style="padding-bottom:0">
  <div class="tabs">
    {#each groups as g, i (g.title)}
      <button class="tab" class:on={activeTab === i} onclick={() => (activeTab = i)}>{g.title}</button>
    {/each}
  </div>
</div>

{#if configError}
  <div class="group" style="padding-top:0"><div style="color:var(--err);font-size:12.5px">操作失败：{configError}</div></div>
{/if}

{#if groups[activeTab]}
  {@const g = groups[activeTab]}
  <div class="group">
    <div class="group-head"><h2>{g.title}</h2><span class="badge mono">{g.ids.length}</span></div>
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
                  {it.status === "configured" ? "已配置" : "未配置"}
                </span>
              </div>
            </div>
            <div>
              {#if id === "git-identity"}
                <span class="mono" style="color:var(--text-3)">弹窗内填写用户名与邮箱</span>
              {:else if id === "npm-dirs"}
                <span class="mono" style="color:var(--text-3)">弹窗内选择目录</span>
              {:else if id === "ssh-key"}
                {#if sshKeys.length}
                  <AppSelect
                    options={sshKeys.map((k) => ({ value: k, label: keyName(k) }))}
                    bind:value={sshSelected}
                    width="100%"
                  />
                {:else}
                  <span class="mono" style="color:var(--text-3)">尚无公钥，点右侧「生成/复用」</span>
                {/if}
              {:else if it.mirrors.length > 0}
                <MirrorPicker mirrors={it.mirrors} bind:value={mirror[id]} />
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
                <AppButton size="sm" onclick={openNpmModal}>设置</AppButton>
              {:else if id === "git-identity"}
                <AppButton size="sm" disabled={runningId === id} onclick={() => (identityModal = true)}>配置</AppButton>
              {:else}
                <AppButton size="sm" disabled={runningId === id} onclick={() => configure(id)}>配置</AppButton>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<div class="group">
  <div class="group-head"><h2>环境变量备份</h2><span class="badge mono">{backups.length}</span></div>
  <div class="card">
    {#each backups as b (b)}
      <div class="row" style="grid-template-columns:1fr auto">
        <span class="mono" style="word-break:break-all">{b}</span>
        <div style="display:flex;gap:8px">
          <AppButton size="sm" onclick={() => viewBackup(b)}>查看</AppButton>
          <AppButton size="sm" onclick={() => restore(b)}>还原</AppButton>
          <AppButton size="sm" variant="danger" onclick={() => delBackup(b)}>删除</AppButton>
        </div>
      </div>
    {/each}
    {#if !backups.length}<div class="row" style="grid-template-columns:1fr"><span style="color:var(--text-3)">暂无备份。执行安装或配置时会自动创建，用于一键还原。</span></div>{/if}
  </div>
</div>

{#if backupView}
  {@const bv = backupView}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>备份内容</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (backupView = null)} title="关闭">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="mono" style="word-break:break-all;color:var(--text-3);margin-bottom:8px">{bv.name}</div>
        <textarea class="field" style="height:280px;width:100%" readonly bind:value={bv.content}></textarea>
        <div style="font-size:12px;color:var(--text-3);margin-top:6px">
          这是环境变量改动前的完整快照（用户级 + 系统级）。点「还原」可恢复到这份内容。
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={async () => await api.copyText(bv.content)}>复制内容</AppButton>
        <AppButton onclick={() => (backupView = null)}>关闭</AppButton>
      </div>
    </div>
  </div>
{/if}

{#if npmModal}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>npm 全局目录与缓存</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (npmModal = false)} title="关闭">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="display:flex;flex-direction:column;gap:14px">
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">全局包目录（prefix）</div>
            <div style="display:flex;gap:8px;align-items:center">
              <span class="mono" style="flex:1;color:var(--text-2);word-break:break-all" title={form.prefix_dir}>{npmLoading ? "读取中…" : (form.prefix_dir || "未选择")}</span>
              <AppButton size="sm" onclick={() => pickDir("prefix_dir")}>选择…</AppButton>
            </div>
          </div>
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">缓存目录（cache）</div>
            <div style="display:flex;gap:8px;align-items:center">
              <span class="mono" style="flex:1;color:var(--text-2);word-break:break-all" title={form.cache_dir}>{npmLoading ? "读取中…" : (form.cache_dir || "未选择")}</span>
              <AppButton size="sm" onclick={() => pickDir("cache_dir")}>选择…</AppButton>
            </div>
          </div>
          <div style="font-size:12px;color:var(--text-3)">
            已显示当前 npm 配置；留空则需手动选择。点击「确定」后写入 npm 配置，并把全局包目录追加到用户级 PATH。改动后，已安装的全局包需重装才会在新目录生效。
          </div>
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={() => (npmModal = false)}>取消</AppButton>
        <AppButton variant="primary" onclick={applyNpmDirs} disabled={runningId === "npm-dirs"}>确定，保存并配置</AppButton>
      </div>
    </div>
  </div>
{/if}

{#if identityModal}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>Git 身份</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (identityModal = false)} title="关闭">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="display:flex;flex-direction:column;gap:14px">
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">用户名（user.name）</div>
            <input class="field" style="width:100%" placeholder="例如：Zhang San" bind:value={form.git_name} />
          </div>
          <div>
            <div style="font-size:12.5px;margin-bottom:6px">邮箱（user.email）</div>
            <input class="field" style="width:100%" placeholder="例如：me@example.com" bind:value={form.git_email} />
          </div>
          <div style="font-size:12px;color:var(--text-3)">
            点击「确定」后写入 Git 全局配置（--global），对当前用户的所有仓库生效。SSH 密钥生成时会使用此邮箱作为备注。
          </div>
        </div>
      </div>
      <div class="modal-foot">
        <AppButton onclick={() => (identityModal = false)}>取消</AppButton>
        <AppButton variant="primary" onclick={applyIdentity} disabled={runningId === "git-identity"}>确定，保存并配置</AppButton>
      </div>
    </div>
  </div>
{/if}
