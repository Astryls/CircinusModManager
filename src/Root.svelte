<script lang="ts">
  // The whole window, inside one boundary.
  //
  // Panel.svelte already catches a panel that fails, but the outermost layer of App -- the title
  // bar, the panes, the footer that reads the snapshot directly -- sits outside every one of
  // them. Svelte caches a thrown $derived and re-throws it to everyone who reads it afterwards,
  // so one bad value in the store took down every panel *and* the unboundaried frame around
  // them, and the window went black with nothing said and nothing written down. A black window
  // is the worst possible error message: it tells the user nothing and leaves us nothing to read.
  import App from "./App.svelte";
  import { api } from "./lib/api";
  // The link, not a copy of it. This screen pasted the URL and was still pointing at a dead
  // invite after the others were updated -- on the one page somebody only ever reads because
  // something has already gone wrong.
  import { DISCORD } from "./lib/types";

  let copied = $state(false);
  let copyFailed = $state<string | null>(null);

  async function copyDiagnostics() {
    copyFailed = null;
    try {
      await navigator.clipboard.writeText(await api.diagnostics());
      copied = true;
      setTimeout(() => (copied = false), 4000);
    } catch (e) {
      copyFailed = e instanceof Error ? e.message : String(e);
    }
  }
  function reload() {
    location.reload();
  }
</script>

<svelte:boundary
  onerror={(e) => {
    const err = e as { message?: string; stack?: string };
    api.logFromTheWindow(`the window failed: ${err?.message ?? String(e)}`, err?.stack).catch(() => {});
  }}
>
  <App />

  {#snippet failed(error, reset)}
    <div class="crashed">
      <div class="box card">
        <h2>Circinus stopped drawing</h2>
        <p>
          Something went wrong in the window. Your mods and your list are untouched: nothing here writes to ModsConfig.xml on its own, and the folders on disk have not
          been changed.
        </p>
        <p class="mono err">{String(error).slice(0, 1200)}</p>
        <div class="acts">
          <button class="btn primary" onclick={reload}>Reload the window</button>
          <button class="btn" onclick={() => reset()}>Try again without reloading</button>
          <button class="btn" onclick={copyDiagnostics}>{copied ? "Copied" : "Copy diagnostics"}</button>
        </div>
        {#if copyFailed}<p class="note">Could not reach the clipboard: {copyFailed}</p>{/if}
        <p class="note">
          Copy the diagnostics and paste them into <a href={DISCORD} target="_blank" rel="noreferrer">the Discord</a>. They carry the version, your
          folders and the end of the log, with your user folder written as ~ so your name does not go with them.
        </p>
      </div>
    </div>
  {/snippet}
</svelte:boundary>

<style>
  .crashed {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--bg);
    overflow: auto;
  }
  .box {
    max-width: 620px;
    width: 100%;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 16px;
  }
  p {
    margin: 0 0 12px;
    color: var(--text-2);
    font-size: 13.5px;
  }
  /* Selectable: the first thing anyone will try is to copy the message itself. */
  .err {
    white-space: pre-wrap;
    user-select: text;
    color: #ffb4ae;
    background: var(--well);
    border-radius: 0;
    padding: 10px 12px;
    font-size: 12px;
    max-height: 220px;
    overflow: auto;
  }
  .acts {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }
  .note {
    font-size: 12px;
    color: var(--text-3);
    margin: 0;
  }
  .note + .note {
    margin-top: 8px;
  }
</style>
