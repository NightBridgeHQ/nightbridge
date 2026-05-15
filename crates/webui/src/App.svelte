<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { appState, connectEvents, disconnectEvents, refreshSnapshot, setToken } from "./state";

  type Tab = "Dashboard" | "Peers" | "Transfers" | "Inbox" | "Settings";

  const tabs: Tab[] = ["Dashboard", "Peers", "Transfers", "Inbox", "Settings"];
  let activeTab: Tab = "Dashboard";
  let tokenInput = "";

  $: state = $appState;
  $: snapshot = state.snapshot;
  $: trustedCount = snapshot?.trustedPeers.length ?? 0;
  $: lanCount = snapshot?.lanPeers.length ?? 0;
  $: inboxCount = snapshot?.inbox.length ?? 0;
  $: transferCount = snapshot?.transfers.length ?? 0;
  $: statusValue = state.connection === "connected" ? "online" : state.connection;
  $: tokenInput = state.token;

  onMount(() => {
    if (state.token) {
      void refreshSnapshot().then(connectEvents);
    }
  });

  onDestroy(() => {
    disconnectEvents();
  });

  function saveToken(): void {
    setToken(tokenInput);
    void refreshSnapshot().then(connectEvents);
  }
</script>

<main class="shell">
  <header class="topbar">
    <div>
      <h1>LocalSend Improved</h1>
      <p>{snapshot?.status.alias ?? "Daemon dashboard"}</p>
    </div>
    <button type="button" on:click={() => void refreshSnapshot().then(connectEvents)}>Refresh</button>
  </header>

  <section class="summary" aria-label="Daemon summary">
    <article>
      <span>Daemon</span>
      <strong>{statusValue}</strong>
      <small>{snapshot?.status.version ?? state.error ?? "Token required"}</small>
    </article>
    <article>
      <span>Peers</span>
      <strong>{trustedCount + lanCount}</strong>
      <small>{trustedCount} trusted, {lanCount} LAN</small>
    </article>
    <article>
      <span>Transfers</span>
      <strong>{transferCount}</strong>
      <small>Active sessions</small>
    </article>
    <article>
      <span>Inbox</span>
      <strong>{inboxCount}</strong>
      <small>Received files</small>
    </article>
  </section>

  <section class="workspace">
    <nav aria-label="Dashboard sections">
      {#each tabs as tab}
        <button
          type="button"
          class:active={activeTab === tab}
          aria-current={activeTab === tab ? "page" : undefined}
          on:click={() => (activeTab = tab)}
        >
          {tab}
        </button>
      {/each}
    </nav>

    <div class="panel">
      {#if activeTab === "Dashboard"}
        <h2>Dashboard</h2>
        <dl class="facts">
          <div><dt>Fingerprint</dt><dd>{snapshot?.status.fingerprint ?? "Not connected"}</dd></div>
          <div><dt>Inbox</dt><dd>{snapshot?.status.inbox_dir ?? "Not loaded"}</dd></div>
          <div><dt>LocalSend</dt><dd>{snapshot?.status.localsend_port ?? "-"}</dd></div>
          <div><dt>Native</dt><dd>{snapshot?.status.native_port ?? "-"}</dd></div>
        </dl>
        {#if state.lastEvent}
          <p class="event">Last event: {state.lastEvent.type ?? "daemon"}</p>
        {/if}
      {:else if activeTab === "Peers"}
        <h2>Peers</h2>
        <div class="table">
          {#each snapshot?.trustedPeers ?? [] as peer}
            <div class="row">
              <span>{peer.label || peer.fingerprint}</span>
              <small>{peer.policy}</small>
            </div>
          {/each}
          {#each snapshot?.lanPeers ?? [] as peer}
            <div class="row">
              <span>{peer.alias}</span>
              <small>{peer.address}:{peer.port}</small>
            </div>
          {/each}
          {#if trustedCount + lanCount === 0}<p class="empty">No peers loaded</p>{/if}
        </div>
      {:else if activeTab === "Transfers"}
        <h2>Transfers</h2>
        {#if transferCount === 0}
          <p class="empty">No active transfers</p>
        {:else}
          <pre>{JSON.stringify(snapshot?.transfers, null, 2)}</pre>
        {/if}
      {:else if activeTab === "Inbox"}
        <h2>Inbox</h2>
        <div class="table">
          {#each snapshot?.inbox ?? [] as entry}
            <div class="row">
              <span>{entry.file_name}</span>
              <small>{entry.size} bytes</small>
            </div>
          {/each}
          {#if inboxCount === 0}<p class="empty">Inbox is empty</p>{/if}
        </div>
      {:else}
        <h2>Settings</h2>
        <form class="settings" on:submit|preventDefault={saveToken}>
          <label for="api-token">API token</label>
          <input id="api-token" type="password" bind:value={tokenInput} autocomplete="off" />
          <button type="submit">Save token</button>
        </form>
        {#if state.error}<p class="error">{state.error}</p>{/if}
      {/if}
    </div>
  </section>
</main>
