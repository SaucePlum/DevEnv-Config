<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import AppButton from "../lib/components/AppButton.svelte";
  import { askConfirm } from "../lib/confirm.svelte";

  let entries = $state<{ name: string; size: number; modified: string }[]>([]);
  let busy = $state(false);

  const totalMb = $derived(entries.reduce((s, e) => s + e.size, 0) / 1048576);
  const fmtSize = (n: number) =>
    n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;

  async function refresh() {
    busy = true;
    try {
      entries = await api.listCacheEntries().catch(() => []);
      store.cacheMb = await api.cacheSizeMb().catch(() => 0);
    } finally {
      busy = false;
    }
  }

  async function del(name: string) {
    if (!(await askConfirm(`删除 ${name}？下次安装同一版本时会重新下载。`, { danger: true }))) return;
    busy = true;
    try { await api.deleteCacheEntry(name); await refresh(); }
    finally { busy = false; }
  }

  async function clearAll() {
    if (!entries.length) return;
    if (!(await askConfirm(`清空全部 ${entries.length} 个缓存文件？下次安装需要重新下载。`, { danger: true }))) return;
    busy = true;
    try { await api.clearCache(); await refresh(); }
    finally { busy = false; }
  }

  onMount(refresh);
</script>

<div class="toolbar">
  <h1>下载管理</h1>
  <div class="stats">
    <span class="stat">文件 <b class="mono">{entries.length}</b></span>
    <span class="stat">占用 <b class="mono">{totalMb.toFixed(1)} MB</b></span>
  </div>
  <div class="tb-actions">
    <AppButton size="sm" onclick={() => api.openCacheDir().catch((e) => alert(String(e)))}>打开缓存目录</AppButton>
    <AppButton size="sm" onclick={refresh} disabled={busy}>刷新</AppButton>
    <AppButton size="sm" variant="danger" onclick={clearAll} disabled={busy || !entries.length}>清空全部</AppButton>
  </div>
</div>

<div class="group">
  <div class="group-head">
    <h2>已缓存的安装包</h2>
    <span class="badge mono">{entries.length}</span>
    <span class="grow"></span>
  </div>
  <div class="card">
    {#each entries as e (e.name)}
      <div class="row" style="grid-template-columns:minmax(0,1fr) 110px 130px auto">
        <span class="mono" style="word-break:break-all">{e.name}</span>
        <span class="mono" style="color:var(--text-3)">{fmtSize(e.size)}</span>
        <span class="mono" style="color:var(--text-3)">{e.modified}</span>
        <div style="display:flex;justify-content:flex-end">
          <AppButton size="sm" variant="danger" onclick={() => del(e.name)} disabled={busy}>删除</AppButton>
        </div>
      </div>
    {/each}
    {#if !entries.length}
      <div class="row" style="grid-template-columns:1fr">
        <span style="color:var(--text-3)">缓存是空的</span>
      </div>
    {/if}
  </div>
</div>
