<script lang="ts">
  import type { VersionInfo } from "../types";
  import { t } from "../i18n.svelte";
  let { versions, value = $bindable(), enabled = true }: { versions: VersionInfo[]; value?: string; enabled?: boolean } = $props();
  let open = $state(false);
  let up = $state(false);
  let btn: HTMLButtonElement;
  let root: HTMLDivElement;

  $effect(() => {
    const rec = versions.find((v) => v.recommended);
    if (rec && !value) value = rec.version;
  });

  // 展开期间：点击菜单外或按 Esc 自动收起
  $effect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = false;
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  });

  function toggle() {
    if (!enabled) return;
    open = !open;
    if (open && btn) {
      const r = btn.getBoundingClientRect();
      up = window.innerHeight - r.bottom < 260;
    }
  }
</script>

<div style="position:relative" bind:this={root}>
  <button class="select" bind:this={btn} disabled={!enabled} onclick={toggle}>
    <span class="mono">{value || "—"}</span>
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 6l4 4 4-4" /></svg>
  </button>
  {#if open}
    <div class="menu" style={up ? "bottom:36px" : "top:36px"}>
      {#each versions as v (v.version)}
        <button class="menu-item" class:on={v.version === value}
          onclick={() => { value = v.version; open = false; }}>
          <span class="mono">{v.version}</span>
          {#if v.lts}<span class="chip">LTS</span>{/if}
          {#if v.recommended}<span class="chip ok">{t("version.recommended")}</span>{/if}
        </button>
      {/each}
      {#if versions.length === 0}
        <div class="menu-item" style="color:var(--text-3)">{t("version.unavailable")}</div>
      {/if}
    </div>
  {/if}
</div>
