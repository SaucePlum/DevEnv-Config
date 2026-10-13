<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { store, setTheme } from "../store.svelte";
  import { i18n, switchLang, t } from "../i18n.svelte";
  import AppLogo from "./AppLogo.svelte";

  // 自定义标题栏（窗口已关闭系统边框 decorations:false）。
  // 拖动与双击最大化由 Tauri 内置的 data-tauri-drag-region 脚本处理，
  // 这里只负责标题栏内容与右侧窗口控制按钮。
  const win = getCurrentWindow();
  let maximized = $state(false);
  let unlistenResize: (() => void) | undefined;

  function toggleTheme() {
    setTheme(store.theme === "light" ? "dark" : "light");
  }
  function toggleLang() {
    switchLang(i18n.lang === "zh" ? "en" : "zh");
  }
  function openBackups() {
    store.page = "config";
  }
  function minimize() {
    win.minimize().catch(() => {});
  }
  function toggleMaximize() {
    win.toggleMaximize().catch(() => {});
  }
  function closeWindow() {
    win.close().catch(() => {});
  }

  onMount(async () => {
    maximized = await win.isMaximized().catch(() => false);
    unlistenResize = await win.onResized(() => {
      win.isMaximized().then((v) => (maximized = v)).catch(() => {});
    });
  });
  onDestroy(() => unlistenResize?.());

  // 语言切换时同步系统窗口标题（任务栏/Alt-Tab 也跟随）
  $effect(() => {
    win.setTitle(t("title.app_name")).catch(() => {});
  });
</script>

<header class="titlebar" data-tauri-drag-region="deep">
  <div class="app-icon"><AppLogo size={20} /></div>
  <span class="app-name">{t("title.app_name")}</span>
  <span class="app-ver mono">v1.0</span>

  <div class="tb-right">
    {#if !store.elevated}
      <span class="pill" title={t("title.not_admin_tip")}>{t("title.not_admin")}</span>
    {/if}
    <span class="pill info mono" title={t("title.cache_tip")}>{store.cacheMb.toFixed(1)} MB</span>
    <button class="pill info" title={t("title.language")} onclick={toggleLang}>{i18n.lang === "zh" ? "中文" : "EN"}</button>
    <button class="icon-btn" title={t("title.backups")} onclick={openBackups}>
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
        <path d="M3 3h7l3 3v7H3z" /><path d="M6 3v4h4V3" />
      </svg>
    </button>
    <button class="icon-btn" title={t("title.theme")} onclick={toggleTheme}>
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
        {#if store.theme === "light"}
          <circle cx="8" cy="8" r="3" /><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M13 3l-1.5 1.5M4.5 11.5L3 13" />
        {:else}
          <path d="M13 9.5A5.5 5.5 0 016.5 3a5.5 5.5 0 106.5 6.5z" />
        {/if}
      </svg>
    </button>
  </div>

  <div class="win-controls">
    <button class="win-btn" title={t("title.minimize")} aria-label={t("title.minimize")} onclick={minimize}>
      <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1" /></svg>
    </button>
    <button class="win-btn" title={maximized ? t("title.restore") : t("title.maximize")} aria-label={maximized ? t("title.restore") : t("title.maximize")} onclick={toggleMaximize}>
      {#if maximized}
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
          <path d="M2.5 2.5V0.5h7v7h-2" /><rect x="0.5" y="2.5" width="7" height="7" />
        </svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
          <rect x="0.5" y="0.5" width="9" height="9" />
        </svg>
      {/if}
    </button>
    <button class="win-btn close" title={t("title.close")} aria-label={t("title.close")} onclick={closeWindow}>
      <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" /></svg>
    </button>
  </div>
</header>
