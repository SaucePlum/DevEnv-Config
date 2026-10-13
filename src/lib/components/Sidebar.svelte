<script lang="ts">
  import { store } from "../store.svelte";

  const pages = [
    { key: "overview", label: "开发环境配置", count: () => store.items.filter((i) => i.kind !== "config").length },
    { key: "config", label: "环境变量配置", count: () => store.items.filter((i) => i.kind === "config").length },
    { key: "download", label: "下载管理", count: () => null },
    { key: "about", label: "关于", count: () => null },
  ] as const;

  const icon = (key: string) => {
    if (key === "overview")
      return `<rect x="2" y="2" width="5" height="5" /><rect x="9" y="2" width="5" height="5" /><rect x="2" y="9" width="5" height="5" /><rect x="9" y="9" width="5" height="5" />`;
    if (key === "config")
      return `<circle cx="8" cy="8" r="2.2" /><path d="M8 1.6v2M8 12.4v2M1.6 8h2M12.4 8h2M3.5 3.5l1.4 1.4M11.1 11.1l1.4 1.4M12.5 3.5l-1.4 1.4M4.9 11.1l-1.4 1.4" />`;
    if (key === "about")
      return `<circle cx="8" cy="8" r="6.2" /><path d="M8 7.2v3.6" /><circle cx="8" cy="5" r="0.5" fill="currentColor" stroke="none" />`;
    return `<path d="M8 2v7.5M4.5 6.5L8 10l3.5-3.5" /><path d="M3 13h10" />`;
  };
</script>

<nav class="sidebar">
  {#each pages as p (p.key)}
    <button class="nav-item" class:on={store.page === p.key} onclick={() => (store.page = p.key)}>
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">{@html icon(p.key)}</svg>
      <span>{p.label}</span>
      {#if p.count() !== null}<span class="ct mono">{p.count()}</span>{/if}
    </button>
  {/each}
</nav>
