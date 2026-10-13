<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { portal } from "../portal";
  import { t } from "../i18n.svelte";
  import AppButton from "./AppButton.svelte";
  import AppSelect from "./AppSelect.svelte";
  import { askConfirm } from "../confirm.svelte";
  let { onRun, keys = [], selected = $bindable("") }: {
    onRun: () => Promise<void>;
    keys?: string[];
    selected?: string;
  } = $props();
  let pubkey = $state("");
  let testOut = $state("");
  let open = $state(false);
  let busy = $state(false);
  // ed25519 密钥现状：决定主按钮文案与行为（生成 / 重建公钥 / 复制公钥）
  let status = $state({ has_public: false, has_private: false });

  const keyName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const keyOptions = $derived(keys.map((k) => ({ value: k, label: keyName(k) })));

  const primaryLabel = $derived(
    status.has_public ? t("ssh.copy_public") : status.has_private ? t("ssh.rebuild_public") : t("ssh.generate")
  );

  async function refreshStatus() {
    status = await api
      .sshKeyStatus()
      .catch(() => ({ has_public: false, has_private: false }));
  }

  onMount(refreshStatus);

  async function loadSelected() {
    if (!selected) return;
    const name = selected.split(/[\\/]/).pop() ?? selected;
    try {
      pubkey = await api.readPublicKey(name);
    } catch (e) {
      testOut = String(e);
    }
    open = true;
  }

  async function primary() {
    // 已有公钥：无需确认，直接展示并复制 id_ed25519.pub
    if (status.has_public) {
      try {
        pubkey = await api.readPublicKey("id_ed25519.pub");
        await api.copyText(pubkey);
        open = true;
      } catch (e) {
        testOut = String(e);
      }
      return;
    }
    const msg = status.has_private
      ? t("ssh.rebuild_confirm")
      : t("ssh.generate_confirm");
    if (!(await askConfirm(msg))) return;
    busy = true;
    try {
      await onRun();
      pubkey = await api.readPublicKey("id_ed25519.pub");
      await api.copyText(pubkey);
      await refreshStatus();
      open = true;
    } catch (e) {
      testOut = String(e);
    } finally {
      busy = false;
    }
  }

  async function test() {
    busy = true;
    testOut = t("ssh.testing");
    try {
      testOut = await api.testSsh();
    } catch (e) {
      testOut = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<AppButton size="sm" onclick={primary} disabled={busy}>{primaryLabel}</AppButton>
<AppButton size="sm" onclick={loadSelected} disabled={!selected}>{t("ssh.view_key")}</AppButton>

{#if open}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>{t("ssh.public_key")}</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (open = false)} title={t("common.close")}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="font-size:12px;color:var(--text-3);margin-bottom:6px">
          {t("ssh.hint")}
        </div>
        {#if keyOptions.length}
          <AppSelect options={keyOptions} bind:value={selected} width="100%" onselect={loadSelected} />
        {:else}
          <div class="mono" style="color:var(--text-3)">{t("ssh.no_pub")}</div>
        {/if}
        <textarea class="field" style="height:120px;width:100%;margin-top:8px" readonly bind:value={pubkey}></textarea>
        <div style="display:flex;gap:8px;margin-top:10px">
          <AppButton size="sm" onclick={async () => await api.copyText(pubkey)} disabled={!pubkey}>{t("ssh.copy_public")}</AppButton>
          <AppButton size="sm" onclick={() => api.openUrl("https://github.com/settings/ssh/new")}>{t("ssh.open_github")}</AppButton>
          <AppButton size="sm" onclick={test} disabled={busy}>{t("ssh.test")}</AppButton>
        </div>
        {#if testOut}<pre class="mono" style="white-space:pre-wrap;margin-top:10px;font-size:12px">{testOut}</pre>{/if}
      </div>
      <div class="modal-foot"><AppButton onclick={() => (open = false)}>{t("common.close")}</AppButton></div>
    </div>
  </div>
{/if}
