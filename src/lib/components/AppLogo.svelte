<script lang="ts">
  // 应用图标（与 src-tauri/icons/app-icon.svg 同源）：
  // 蓝色圆角底板 + 白色立方体「软件包」+ 绿色完成徽章，象征「开发环境一键配好」。
  import { t } from "../i18n.svelte";
  let { size = 24 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 1024 1024"
  xmlns="http://www.w3.org/2000/svg"
  role="img"
  aria-label={t("title.app_name")}
>
  <defs>
    <linearGradient id="appLogoBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#3AA8F0" />
      <stop offset="1" stop-color="#0067C0" />
    </linearGradient>
    <linearGradient id="appLogoSheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.20" />
      <stop offset="0.55" stop-color="#FFFFFF" stop-opacity="0" />
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" rx="230" fill="url(#appLogoBg)" />
  <rect width="1024" height="1024" rx="230" fill="url(#appLogoSheen)" />
  <polygon points="512,188 806,352 512,516 218,352" fill="#FFFFFF" opacity="0.97" />
  <polygon points="218,352 512,516 512,806 218,642" fill="#FFFFFF" opacity="0.80" />
  <polygon points="806,352 512,516 512,806 806,642" fill="#D6EBFF" opacity="0.88" />
  <line x1="512" y1="516" x2="512" y2="806" stroke="#B7DBFF" stroke-width="18" opacity="0.9" />
  <circle cx="748" cy="748" r="132" fill="#2FBF6B" stroke="#FFFFFF" stroke-width="26" />
  <path
    d="M690 750l42 42 78-90"
    stroke="#FFFFFF"
    stroke-width="30"
    stroke-linecap="round"
    stroke-linejoin="round"
    fill="none"
  />
</svg>
