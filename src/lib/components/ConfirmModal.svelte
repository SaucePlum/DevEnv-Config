<script lang="ts">
  import { portal } from "../portal";
  import { t } from "../i18n.svelte";
  import AppButton from "./AppButton.svelte";
  import { confirmState, answerConfirm } from "../confirm.svelte";
</script>

{#if confirmState.open}
  <div class="scrim" use:portal style="z-index:60">
    <div class="modal" style="width:min(440px, 90vw)">
      <div class="modal-head"><h3>{confirmState.title}</h3></div>
      <div class="modal-body">
        <div style="white-space:pre-wrap">{confirmState.message}</div>
      </div>
      <div class="modal-foot">
        {#if confirmState.mode === "confirm"}
          <AppButton onclick={() => answerConfirm(false)}>{t("common.cancel")}</AppButton>
          <AppButton
            variant={confirmState.danger ? "danger" : "primary"}
            onclick={() => answerConfirm(true)}
          >
            {t("common.ok")}
          </AppButton>
        {:else}
          <AppButton onclick={() => answerConfirm(true)}>{t("common.ok")}</AppButton>
        {/if}
      </div>
    </div>
  </div>
{/if}
