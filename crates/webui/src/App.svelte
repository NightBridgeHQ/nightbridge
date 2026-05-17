<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    appState,
    connectEvents,
    disconnectEvents,
    loadDesktopSettings,
    refreshSnapshot,
    saveDesktopSettings,
    setToken,
    startStandaloneDaemon,
    stopStandaloneDaemon,
    type GuiMode
  } from "./state";

  type Tab = "Dashboard" | "Peers" | "Transfers" | "Inbox" | "Settings";

  const tabs: Tab[] = ["Dashboard", "Peers", "Transfers", "Inbox", "Settings"];
  let activeTab: Tab = "Dashboard";
  let tokenInput = "";
  let endpointInput = "";
  let modeInput: GuiMode = "remote";

  $: state = $appState;
  $: snapshot = state.snapshot;
  $: trustedCount = snapshot?.trustedPeers.length ?? 0;
  $: lanCount = snapshot?.lanPeers.length ?? 0;
  $: inboxCount = snapshot?.inbox.length ?? 0;
  $: transferCount = snapshot?.transfers.length ?? 0;
  $: statusValue = state.connection === "connected" ? "online" : state.connection;
  $: standaloneLabel = state.guiMode === "standalone" ? `standalone ${state.standalone}` : "remote daemon";
  $: tokenInput = state.token;
  $: endpointInput = state.apiBase;
  $: modeInput = state.guiMode;
  $: showDesktopSetup = state.isDesktop && (!state.token || (state.guiMode === "remote" && !state.apiBase));

  onMount(() => {
    void loadDesktopSettings().then(() => refreshSnapshot()).then(connectEvents);
  });

  onDestroy(() => {
    disconnectEvents();
  });

  function saveToken(): void {
    setToken(tokenInput);
    void refreshSnapshot().then(connectEvents);
  }

  function saveDesktopConnection(): void {
    if (modeInput === "standalone") {
      void saveDesktopSettings(modeInput, "", "")
        .then(startStandaloneDaemon)
        .then(refreshSnapshot)
        .then(connectEvents);
      return;
    }

    void saveDesktopSettings(modeInput, endpointInput, tokenInput).then(refreshSnapshot).then(connectEvents);
  }

  function stopStandalone(): void {
    void stopStandaloneDaemon();
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

  {#if showDesktopSetup}
    <section class="desktop-setup" aria-label="Desktop connection setup">
      <div class="mode-switch" role="group" aria-label="Desktop mode">
        <button
          type="button"
          class:active={modeInput === "remote"}
          on:click={() => (modeInput = "remote")}
        >
          Remote daemon
        </button>
        <button
          type="button"
          class:active={modeInput === "standalone"}
          on:click={() => (modeInput = "standalone")}
        >
          Standalone local daemon
        </button>
      </div>

      {#if modeInput === "remote"}
        <label for="remote-endpoint">Daemon endpoint</label>
        <input id="remote-endpoint" type="url" placeholder="http://127.0.0.1:53317" bind:value={endpointInput} />
      {/if}

      {#if modeInput === "remote"}
        <label for="desktop-api-token">API token</label>
        <input id="desktop-api-token" type="password" bind:value={tokenInput} autocomplete="off" />
      {:else}
        <p class="event">Standalone daemon: {state.standalone}</p>
      {/if}

      <div class="actions">
        <button type="button" on:click={saveDesktopConnection}>
          {modeInput === "standalone" ? "Start" : "Connect"}
        </button>
        {#if state.guiMode === "standalone" && state.standalone === "running"}
          <button type="button" on:click={stopStandalone}>Stop</button>
        {/if}
      </div>
      {#if state.error}<p class="error">{state.error}</p>{/if}
    </section>
  {/if}

  <section class="summary" aria-label="Daemon summary">
    <article>
      <span>Daemon</span>
      <strong>{statusValue}</strong>
      <small>{snapshot?.status.version ?? state.error ?? standaloneLabel}</small>
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
          {#if state.isDesktop}
            <label for="settings-mode">Desktop mode</label>
            <select id="settings-mode" bind:value={modeInput}>
              <option value="remote">Remote daemon</option>
              <option value="standalone">Standalone local daemon</option>
            </select>
            {#if modeInput === "remote"}
              <label for="settings-endpoint">Daemon endpoint</label>
              <input id="settings-endpoint" type="url" bind:value={endpointInput} autocomplete="off" />
            {:else}
              <p class="event">Standalone daemon: {state.standalone}</p>
            {/if}
          {/if}
          {#if !state.isDesktop || modeInput === "remote"}
            <label for="api-token">API token</label>
            <input id="api-token" type="password" bind:value={tokenInput} autocomplete="off" />
          {/if}
          {#if state.isDesktop}
            <button type="button" on:click={saveDesktopConnection}>Save connection</button>
            {#if state.guiMode === "standalone" && state.standalone === "running"}
              <button type="button" on:click={stopStandalone}>Stop standalone daemon</button>
            {/if}
          {:else}
            <button type="submit">Save token</button>
          {/if}
        </form>
        {#if state.error}<p class="error">{state.error}</p>{/if}
      {/if}
    </div>
  </section>
</main>
