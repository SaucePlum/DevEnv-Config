<script lang="ts">
  import { t } from "../i18n.svelte";
  // 自定义下拉框：替代原生 <select>，支持外部点击/Esc 收起、空间不足时向上展开
  let {
    options,
    value = $bindable(),
    disabled = false,
    placeholder = "",
    width = "auto",
    onselect,
    display,
  }: {
    options: { value: string; label: string }[];
    value?: string;
    disabled?: boolean;
    placeholder?: string;
    width?: string;
    onselect?: () => void;
    /** 覆盖按钮上显示的文案（值不在 options 里时用，例如「未换源」） */
    display?: string;
  } = $props();

  let open = $state(false);
  let up = $state(false);
  let root: HTMLDivElement;

  const label = $derived(display ?? options.find((o) => o.value === value)?.label ?? value ?? (placeholder || t("select.placeholder")));

  $effect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = false;
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  });

  function toggle() {
    if (disabled) return;
    open = !open;
    if (open && root) {
      const r = root.getBoundingClientRect();
      up = window.innerHeight - r.bottom < 260;
    }
  }

  function pick(v: string) {
    value = v;
    open = false;
    onselect?.();
  }
</script>

<div style="position:relative;{width !== 'auto' ? `width:${width}` : ''}" bind:this={root}>
  <!-- 按钮填满容器：否则展开菜单锚定到容器右缘，与按钮左对齐的内容错位 -->
  <button class="select" style="width:100%" disabled={disabled} onclick={toggle}>
    <span class="mono">{label}</span>
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 6l4 4 4-4" /></svg>
  </button>
  {#if open}
    <!-- 覆盖全局 .menu 的 right:0/width:200px，改为与容器等宽并对齐 -->
    <div class="menu" style="{up ? 'bottom:36px' : 'top:36px'};left:0;right:0;width:auto">
      {#each options as o (o.value)}
        <button class="menu-item" class:on={o.value === value} onclick={() => pick(o.value)}>
          <span class="mono" style="word-break:break-all;text-align:left">{o.label}</span>
        </button>
      {/each}
      {#if options.length === 0}
        <div class="menu-item" style="color:var(--text-3)">{t("select.empty")}</div>
      {/if}
    </div>
  {/if}
</div>
