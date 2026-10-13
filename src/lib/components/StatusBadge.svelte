<script lang="ts">
  import type { DetectStatus, Phase } from "../types";
  import type { MsgKey } from "../i18n/messages";
  import { t } from "../i18n.svelte";
  let { status, phase, message = "" }: { status?: DetectStatus; phase?: Phase; message?: string } = $props();

  const labelFor = (s: DetectStatus) => t(`status.${s}` as MsgKey);
  const phaseLabel = (p: Phase) => t(`phase.${p}` as MsgKey);

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
  {phase ? phaseLabel(phase) : status ? labelFor(status) : ""}
</span>
