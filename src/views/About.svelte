<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import AppLogo from "../lib/components/AppLogo.svelte";

  let version = $state("");
  let elevated = $state(false);

  onMount(async () => {
    version = await api.appVersion().catch(() => "");
    elevated = await api.isElevated().catch(() => false);
  });

  const features = [
    "一键安装 20+ 开发工具：运行环境、编译工具链、数据库、Web 服务与 AI 编程工具",
    "全部软件默认选最新稳定版，国内提供镜像回落，下载带校验和与缓存",
    "环境变量写入前自动备份，一键还原；路径写入前做规范化去重",
    "十几种包管理器换源：npm、pnpm、pip、Cargo、Go、Maven、Composer 等",
    "已安装检测以注册表为准，自定义安装盘符与目录也能正确识别",
  ];
</script>

<div class="about-wrap">
  <div class="group">
    <div class="card" style="padding:22px">
      <div style="display:flex;align-items:center;gap:14px">
        <AppLogo size={48} />
        <div>
          <div style="font-size:18px;font-weight:700">开发环境快速配置助手</div>
          <div class="mono" style="color:var(--text-3);font-size:12.5px">版本 {version || "…"} · 运行在 Windows · MIT 开源协议</div>
        </div>
      </div>
      <p style="margin:14px 0 0;color:var(--text-2);font-size:13px;line-height:1.7">
        为 Windows 开发者准备的一站式环境组装工具：选好要装的软件，剩下的下载、安装引导、环境变量、换源配置全部由工具完成。
      </p>
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>它能做什么</h2></div>
    <div class="card">
      {#each features as f, i (i)}
        <div class="row" style="grid-template-columns:minmax(0,1fr)">
          <span style="color:var(--text-2);font-size:13px">{f}</span>
        </div>
      {/each}
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>开源</h2></div>
    <div class="card">
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto">
        <span style="font-size:13px;color:var(--text-2)">本项目以 MIT 协议开源，源码可自由查看、修改与分发</span>
      </div>
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto">
        <span style="font-size:13px;color:var(--text-3)">完整源码与构建说明见仓库中的 README.md；运行日志仅保存在本机，不会上传任何数据</span>
      </div>
    </div>
  </div>

  <div class="group">
    <div class="group-head"><h2>关于作者</h2></div>
    <div class="card">
      <div class="row" style="grid-template-columns:minmax(0,1fr)">
        <div>
          <div style="font-size:13.5px;font-weight:600">小念</div>
          <div style="font-size:12.5px;color:var(--text-3);margin-top:4px">
            开发者与维护者。遇到问题或想提功能建议，欢迎到开源仓库提 Issue。
          </div>
        </div>
      </div>
      <div class="row" style="grid-template-columns:minmax(0,1fr)">
        <span class="mono" style="font-size:12px;color:var(--text-3)">
          当前以{elevated ? "管理员" : "普通"}权限运行；软件包默认从官网拉取，失败自动回落国内镜像
        </span>
      </div>
    </div>
  </div>

  <div style="text-align:center;color:var(--text-3);font-size:12px;margin-top:8px">
    用心打磨，让环境配置不再繁琐
  </div>
</div>

<style>
  /* 自适应填满主区宽度：卡片与其他页面一致，跟随窗口拉伸 */
  .about-wrap { width: 100%; }
</style>
