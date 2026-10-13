<script lang="ts">
  import type { Mirror } from "../types";
  import AppSelect from "./AppSelect.svelte";
  let { mirrors, value = $bindable() }: { mirrors: Mirror[]; value?: string } = $props();
  $effect(() => {
    if (!value && mirrors.length) value = mirrors[0].url;
  });
  const options = $derived(mirrors.map((m) => ({ value: m.url, label: m.name })));
</script>

<AppSelect {options} bind:value width="170px" />

