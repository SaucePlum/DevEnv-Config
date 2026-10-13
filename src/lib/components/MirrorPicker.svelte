<script lang="ts">
  import type { Mirror } from "../types";
  import { t } from "../i18n.svelte";
  import AppSelect from "./AppSelect.svelte";
  let { mirrors, value = $bindable(), current }: { mirrors: Mirror[]; value?: string; current?: string } = $props();

  // 用户是否手动选过：选过之后按钮就显示所选源，不再显示「未换源」
  let picked = $state(false);

  // 地址比较：忽略首尾空白、结尾斜杠与大小写（pnpm 回显带结尾斜杠，直接相等会判错）
  const norm = (s: string) => s.trim().replace(/\/+$/, "").toLowerCase();
  const matched = $derived(current ? mirrors.find((m) => norm(m.url) === norm(current)) : undefined);
  // 本机生效的不是清单里的镜像（如 yarn 默认的官方源），且用户还没重新选择时，明确显示「未换源」
  const display = $derived(current && !matched && !picked ? t("config.mirror_none") : undefined);

  // 读回：本机实际生效的源在清单里时以它为准，每次重新检测后都重新对齐
  $effect(() => {
    if (matched) value = matched.url;
  });
  // 识别不出镜像源时给个默认的待应用目标，避免点「配置」时缺参数
  $effect(() => {
    if (!matched && !value && mirrors.length) value = mirrors[0].url;
  });

  const options = $derived(mirrors.map((m) => ({ value: m.url, label: m.name })));
</script>

<AppSelect {options} bind:value {display} width="170px" onselect={() => (picked = true)} />
