<script lang="ts">
  import { store } from "../store.svelte";
  import { t } from "../i18n.svelte";
  import { portal } from "../portal";
  import AppButton from "./AppButton.svelte";
  let { id, name, onClose }: { id: string; name: string; onClose: () => void } = $props();

  const live = $derived(store.live[id]);
  const pct = $derived(Math.round((live?.progress ?? 0) * 100));
  const done = $derived(live?.phase === "done" || live?.phase === "failed");
</script>

<div class="scrim" use:portal>
  <div class="modal">
    <div class="modal-head">
      <h3>{name}</h3>
      <button class="icon-btn" onclick={onClose} disabled={!done} title={done ? t("common.close") : t("progress.close_running")}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
    <div class="modal-body">
      <div class="mono" style="margin-bottom:8px">{live?.phase ?? t("progress.preparing")} · {live?.message ?? ""}</div>
      <div class="bar"><div class="bar-in" style="width:{pct}%"></div></div>
      <div class="mono" style="color:var(--text-3);margin-top:6px">{pct}%</div>
      {#if live?.attempted?.length}
        <div style="margin-top:10px;font-size:12px;color:var(--text-3)">
          {t("progress.attempted")}
          {#each live.attempted as u}<div class="mono" style="word-break:break-all">{u}</div>{/each}
        </div>
      {/if}
    </div>
    <div class="modal-foot">
      <AppButton onclick={onClose} disabled={!done}>{t("common.close")}</AppButton>
    </div>
  </div>
</div>
