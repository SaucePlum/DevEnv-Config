<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import type { MsgKey } from "../lib/i18n/messages";
  import AppLogo from "../lib/components/AppLogo.svelte";

  let version = $state("");
  let elevated = $state(false);

  onMount(async () => {
    version = await api.appVersion().catch(() => "");
    elevated = await api.isElevated().catch(() => false);
  });

  const features: MsgKey[] = [
    "about.feature1",
    "about.feature2",
    "about.feature3",
    "about.feature4",
    "about.feature5",
  ];
</script>

<div class="about-wrap">
  <div class="group">
    <div class="card" style="padding:22px">
      <div style="display:flex;align-items:center;gap:14px">
        <AppLogo size={48} />
        <div>
          <div style="font-size:18px;font-weight:700">{t("about.app_name")}</div>
          <div class="mono" style="color:var(--text-3);font-size:12.5px">{t("about.version_line", { v: version || "…" })}</div>
        </div>
      </div>
      <p style="margin:14px 0 0;color:var(--text-2);font-size:13px;line-height:1.7">
        {t("about.desc")}
      </p>
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>{t("about.features_title")}</h2></div>
    <div class="card">
      {#each features as f, i (i)}
        <div class="row" style="grid-template-columns:minmax(0,1fr)">
          <span style="color:var(--text-2);font-size:13px">{t(f)}</span>
        </div>
      {/each}
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>{t("about.opensource_title")}</h2></div>
    <div class="card">
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto">
        <span style="font-size:13px;color:var(--text-2)">{t("about.opensource1")}</span>
      </div>
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto">
        <span style="font-size:13px;color:var(--text-3)">{t("about.opensource2")}</span>
      </div>
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>{t("about.author_title")}</h2></div>
    <div class="card">
      <div class="row" style="grid-template-columns:minmax(0,1fr)">
        <div>
          <div style="font-size:13.5px;font-weight:600">{t("about.author_name")}</div>
          <div style="font-size:12.5px;color:var(--text-3);margin-top:4px">
            {t("about.author_desc")}
          </div>
        </div>
      </div>
      <div class="row" style="grid-template-columns:minmax(0,1fr)">
        <span class="mono" style="font-size:12px;color:var(--text-3)">
          {t("about.elevated", { level: elevated ? t("about.level_admin") : t("about.level_normal") })}
        </span>
      </div>
    </div>
  </div>

  <div style="text-align:center;color:var(--text-3);font-size:12px;margin-top:8px">
    {t("about.footer")}
  </div>
</div>

<style>
  /* 自适应填满主区宽度：卡片与其他页面一致，跟随窗口拉伸 */
  .about-wrap { width: 100%; }
</style>
