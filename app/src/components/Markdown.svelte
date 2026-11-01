<script lang="ts">
  // Session text as a safe markdown subset (story 4.20, D-1045): the tree
  // `lib/markdown.ts` reads, written element by element, every string a
  // text node (`{text}`). No HTML is ever parsed, no anchor and no image
  // is made: a link is its text then its address, an image its alt text.
  // Headings are bold lines, not big headings, in a bubble's small room.
  import { parseMarkdown, type Block, type Inline } from "../lib/markdown";

  interface Props {
    /** Untrusted session text. */
    text: string;
  }
  let { text }: Props = $props();

  let blocks = $derived(parseMarkdown(text));
</script>

{#snippet inlines(xs: Inline[])}
  {#each xs as x, i (i)}
    {#if x.kind === "text"}{x.text}{:else if x.kind === "break"}<br />{:else if x.kind === "code"}<code>{x.text}</code
      >{:else if x.kind === "strong"}<strong>{@render inlines(x.children)}</strong>{:else if x.kind === "em"}<em
        >{@render inlines(x.children)}</em
      >{:else if x.kind === "link"}<span class="link" data-md-link
        ><span class="link-text">{@render inlines(x.children)}</span> <span class="url">({x.url})</span></span
      >{:else if x.kind === "image"}<span class="image" data-md-image>[image: {x.alt}]</span>{/if}
  {/each}
{/snippet}

{#snippet render(bs: Block[])}
  {#each bs as b, i (i)}
    {#if b.kind === "paragraph"}
      <p>{@render inlines(b.inlines)}</p>
    {:else if b.kind === "heading"}
      <p class="heading"><strong>{@render inlines(b.inlines)}</strong></p>
    {:else if b.kind === "code"}
      <!-- Its info string, the language, as a caption: text like the rest. -->
      {#if b.info}<span class="code-info">{b.info}</span>{/if}
      <pre><code>{b.text}</code></pre>
    {:else if b.kind === "list"}
      {#if b.ordered}
        <ol start={b.start}>
          {#each b.items as item, n (n)}<li style:margin-left="{item.depth * 16}px">{@render inlines(item.inlines)}</li>{/each}
        </ol>
      {:else}
        <ul>
          {#each b.items as item, n (n)}<li style:margin-left="{item.depth * 16}px">{@render inlines(item.inlines)}</li>{/each}
        </ul>
      {/if}
    {:else if b.kind === "quote"}
      <blockquote>{@render render(b.blocks)}</blockquote>
    {:else if b.kind === "table"}
      <div class="table" data-md-table>
        <table>
          <thead>
            <tr>
              {#each b.header as cell, c (c)}<th style:text-align={b.align[c] ?? undefined}>{@render inlines(cell)}</th>{/each}
            </tr>
          </thead>
          <tbody>
            {#each b.rows as row, r (r)}
              <tr>
                {#each row as cell, c (c)}<td style:text-align={b.align[c] ?? undefined}>{@render inlines(cell)}</td>{/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if b.kind === "rule"}
      <hr />
    {/if}
  {/each}
{/snippet}

<div class="markdown">{@render render(blocks)}</div>

<style>
  .markdown {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .markdown p,
  .markdown ul,
  .markdown ol,
  .markdown pre,
  .markdown blockquote,
  .markdown hr {
    margin: 0;
  }
  .markdown ul,
  .markdown ol {
    padding-left: 20px;
  }
  /* Inside the exchange's own list, a browser would mark these as nested. */
  .markdown ul {
    list-style: disc;
  }
  .markdown ol {
    list-style: decimal;
  }
  .markdown code {
    padding: 0 4px;
    border-radius: var(--radius-control);
    background: var(--color-surface);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .markdown pre {
    padding: 8px 10px;
    border-radius: var(--radius-control);
    background: var(--color-surface);
    overflow-x: auto;
    overflow-wrap: normal;
  }
  .markdown pre code {
    padding: 0;
    background: none;
    white-space: pre;
  }
  .markdown blockquote {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-left: 10px;
    border-left: 3px solid var(--color-border);
    color: var(--color-text-muted);
  }
  .markdown hr {
    border: 0;
    border-top: 1px solid var(--color-border);
  }
  .markdown .code-info {
    margin-bottom: -4px;
    color: var(--color-text-muted);
    font-size: 12px;
  }
  .markdown .url {
    color: var(--color-text-muted);
    font-size: 12.5px;
  }
  .markdown .link-text {
    text-decoration: underline dotted;
  }
  .markdown .image {
    color: var(--color-text-muted);
    font-style: italic;
  }
  /* A table as a table, scrolled sideways inside the bubble when wide. */
  .markdown .table {
    max-width: 100%;
    overflow-x: auto;
  }
  .markdown table {
    border-collapse: collapse;
    font-size: 13px;
    overflow-wrap: normal;
  }
  .markdown th,
  .markdown td {
    padding: 3px 8px;
    border: 1px solid var(--color-border);
    text-align: left;
    white-space: nowrap;
  }
  .markdown th {
    font-weight: 700;
  }
</style>
