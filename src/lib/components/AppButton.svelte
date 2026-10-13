<script lang="ts">
  import type { Snippet } from "svelte";
  // 自定义按钮：统一 .btn 样式，变体 default / primary / danger，尺寸 md / sm
  let {
    variant = "default",
    size = "md",
    disabled = false,
    title = "",
    onclick,
    children,
  }: {
    variant?: "default" | "primary" | "danger";
    size?: "md" | "sm";
    disabled?: boolean;
    title?: string;
    onclick?: () => void;
    children?: Snippet;
  } = $props();
</script>

<button
  class="btn {variant === 'primary' ? 'pri' : variant === 'danger' ? 'dan' : ''} {size === 'sm' ? 'sm' : ''}"
  {disabled}
  {title}
  {onclick}
>
  {@render children?.()}
</button>
