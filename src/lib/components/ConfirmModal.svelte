<script lang="ts">
  import { portal } from "../portal";
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
          <AppButton onclick={() => answerConfirm(false)}>取消</AppButton>
          <AppButton
            variant={confirmState.danger ? "danger" : "primary"}
            onclick={() => answerConfirm(true)}
          >
            确定
          </AppButton>
        {:else}
          <AppButton onclick={() => answerConfirm(true)}>确定</AppButton>
        {/if}
      </div>
    </div>
  </div>
{/if}
