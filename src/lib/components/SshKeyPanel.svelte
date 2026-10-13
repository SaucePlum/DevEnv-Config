<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { portal } from "../portal";
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
    status.has_public ? "复制公钥" : status.has_private ? "重建公钥" : "生成 ed25519 密钥"
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
      ? "已存在 ed25519 私钥但缺少公钥文件，将从私钥重建公钥（不会改动私钥）。"
      : "生成 ed25519 密钥？生成的私钥无口令保护；已有同名密钥会直接复用，不会覆盖。其他密钥（如 id_rsa）不受影响。";
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
    testOut = "正在测试…";
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
<AppButton size="sm" onclick={loadSelected} disabled={!selected}>查看密钥</AppButton>

{#if open}
  <div class="scrim" use:portal>
    <div class="modal">
      <div class="modal-head"><h3>SSH 公钥</h3><span class="grow"></span>
        <button class="icon-btn" onclick={() => (open = false)} title="关闭">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
      <div class="modal-body">
        <div style="font-size:12px;color:var(--text-3);margin-bottom:6px">
          私钥无口令保护；生成时已有同名密钥则复用，不会覆盖。行上的下拉框也可以切换查看。
        </div>
        {#if keyOptions.length}
          <AppSelect options={keyOptions} bind:value={selected} width="100%" onselect={loadSelected} />
        {:else}
          <div class="mono" style="color:var(--text-3)">~/.ssh 下暂无 .pub 公钥</div>
        {/if}
        <textarea class="field" style="height:120px;width:100%;margin-top:8px" readonly bind:value={pubkey}></textarea>
        <div style="display:flex;gap:8px;margin-top:10px">
          <AppButton size="sm" onclick={async () => await api.copyText(pubkey)} disabled={!pubkey}>复制公钥</AppButton>
          <AppButton size="sm" onclick={() => api.openUrl("https://github.com/settings/ssh/new")}>打开 GitHub 设置页</AppButton>
          <AppButton size="sm" onclick={test} disabled={busy}>测试连接</AppButton>
        </div>
        {#if testOut}<pre class="mono" style="white-space:pre-wrap;margin-top:10px;font-size:12px">{testOut}</pre>{/if}
      </div>
      <div class="modal-foot"><AppButton onclick={() => (open = false)}>关闭</AppButton></div>
    </div>
  </div>
{/if}
