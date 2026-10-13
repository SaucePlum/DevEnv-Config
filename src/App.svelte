<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api, onItemState, onLog } from "./lib/api";
  import { store, applyItemEvent, pushLog, setTheme } from "./lib/store.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Overview from "./views/Overview.svelte";
  import EnvConfig from "./views/EnvConfig.svelte";
  import Downloads from "./views/Downloads.svelte";
  import About from "./views/About.svelte";
  import LogDrawer from "./lib/components/LogDrawer.svelte";
  import ConfirmModal from "./lib/components/ConfirmModal.svelte";
  import AppButton from "./lib/components/AppButton.svelte";

  let unlisten: Array<() => void> = [];
  let loading = $state(true);
  let loadError = $state("");

  // 前端运行时异常进日志抽屉，用户可自助查看
  function reportError(message: string) {
    pushLog({ ts: new Date().toLocaleTimeString("zh-CN", { hour12: false }), level: "error", message });
  }

  async function load() {
    loadError = "";
    // 仅首次（列表为空）显示整页加载态；重新检测原地更新，不再整页刷新
    if (!store.items.length) loading = true;
    // 清掉上次拉取失败的版本列表，让「重新检测」重试它们；成功的使用缓存
    store.versionError = {};
    try { store.items = await api.listItems(); }
    catch (e) { loadError = String(e); }
    finally { loading = false; }
  }

  onMount(async () => {
    window.addEventListener("error", (e) => reportError(`界面错误：${e.message}`));
    window.addEventListener("unhandledrejection", (e) => reportError(`未处理的异常：${String((e as PromiseRejectionEvent).reason)}`));
    setTheme("light");
    store.elevated = await api.isElevated().catch(() => true);
    store.cacheMb = await api.cacheSizeMb().catch(() => 0);
    unlisten.push(await onItemState(applyItemEvent));
    unlisten.push(await onLog(pushLog));
    await load();
  });

  onDestroy(() => unlisten.forEach((f) => f()));
</script>

<div class="app">
  <TitleBar />
  <Sidebar />
  <main class="main">
    {#if loading}
      <div style="padding:40px" class="mono">正在检测本机环境…</div>
    {:else if loadError}
      <div style="padding:40px;color:var(--err)">检测失败：{loadError}<span style="margin-left:12px;display:inline-block"><AppButton size="sm" onclick={load}>重试</AppButton></span></div>
    {:else if store.page === "overview"}
      <Overview onReload={load} />
    {:else if store.page === "config"}
      <EnvConfig onReload={load} />
    {:else if store.page === "about"}
      <About />
    {:else}
      <Downloads />
    {/if}
  </main>
  <LogDrawer />
  <ConfirmModal />
</div>
