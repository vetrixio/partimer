<!--
  Entry content as shown in lists: lines starting with "- " are bullets, other lines paragraphs.
  Text nodes only (never {@html}); the content is the user's own text.
-->
<script lang="ts">
  import { contentBlocks } from "./content";

  let { content }: { content: string } = $props();
</script>

{#each contentBlocks(content) as block, i (i)}
  {#if block.kind === "list"}
    <ul class="bullets">
      {#each block.items as item, j (j)}
        <li>{item}</li>
      {/each}
    </ul>
  {:else}
    <p class="para">{block.text}</p>
  {/if}
{/each}

<style>
  .bullets {
    margin: 2px 0 0;
    padding-left: 1.2em;
  }
  .bullets,
  .para {
    font-size: 0.9em;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .para {
    margin: 2px 0 0;
  }
</style>
