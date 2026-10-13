<script lang="ts">
  import type { DetectStatus, Phase } from "../types";
  let { status, phase, message = "" }: { status?: DetectStatus; phase?: Phase; message?: string } = $props();

  const labelFor = (s: DetectStatus) =>
    ({ installed: "已安装", not_installed: "未安装", configured: "已配置", not_configured: "未配置" }[s]);

  const phaseLabel: Record<Phase, string> = {
    not_installed: "未安装", installed: "已安装", drift: "版本不一致",
    downloading: "下载中", verifying_hash: "校验中", waiting_user: "等待操作", wizard_running: "安装中",
    extracting: "解压中", moving: "移动中", finalizing: "收尾中", verifying: "验证中",
    done: "已完成", failed: "失败", not_configured: "未配置", configured: "已配置",
  };

  const kind = $derived(
    phase
      ? phase === "done"
        ? "ok"
        : phase === "failed"
          ? "err"
          : "busy"
      : status === "installed" || status === "configured"
        ? "ok"
        : "idle"
  );
</script>

<span class="pill {kind === 'ok' ? 'ok' : kind === 'err' ? 'err' : kind === 'busy' ? 'info' : ''}" title={message}>
  {phase ? phaseLabel[phase] : status ? labelFor(status) : ""}
</span>
