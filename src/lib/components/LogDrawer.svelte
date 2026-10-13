<script lang="ts">
  import { store } from "../store.svelte";
  import { t } from "../i18n.svelte";
  import AppButton from "./AppButton.svelte";
  let open = $state(false);
  let box = $state<HTMLDivElement | null>(null);

  $effect(() => {
    store.logs.length; // 追踪日志变化，新日志到达时滚到底部
    if (open && box) box.scrollTop = box.scrollHeight;
  });
</script>

<button class="log-fab" onclick={() => (open = !open)} title={t("log.title")}>
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
    <path d="M3 3h10v10H3z" /><path d="M5 6h6M5 8.5h6M5 11h4" />
  </svg>
</button>

{#if open}
  <aside class="drawer">
    <div class="modal-head">
      <h3>{t("log.title")}</h3>
      <span class="grow"></span>
      <AppButton size="sm" onclick={() => (store.logs = [])}>{t("log.clear")}</AppButton>
      <button class="icon-btn" onclick={() => (open = false)} title={t("common.close")}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
    <div class="drawer-body" bind:this={box}>
      {#each store.logs as l (l.seq ?? l.ts + l.message)}
        <div class="log-line {l.level}">
          <span class="mono" style="color:var(--text-3)">{l.ts}</span>
          {#if l.item}<span class="chip mono">{l.item}</span>{/if}
          <span>{l.message}</span>
        </div>
      {/each}
      {#if !store.logs.length}<div style="color:var(--text-3)">{t("log.empty")}</div>{/if}
    </div>
  </aside>
{/if}
