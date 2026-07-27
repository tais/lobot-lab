<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import ItemPicker from "./lib/ItemPicker.svelte";
  import type {
    Animation,
    AttachmentOption,
    Attachments,
    Audit,
    Inventory,
    Preview,
    PreviewContext,
    PreviewRequest,
    ScenarioState,
    WorkspaceSummary
  } from "./lib/types";

  const slots = [
    ["HANDPOS", "Primary hand"],
    ["SECONDHANDPOS", "Off hand"],
    ["HELMETPOS", "Helmet"],
    ["VESTPOS", "Vest"],
    ["LEGPOS", "Leg armour"],
    ["HEAD1POS", "Face slot 1"],
    ["HEAD2POS", "Face slot 2"],
    ["VESTPOCKPOS", "Vest LBE"],
    ["LTHIGHPOCKPOS", "Left thigh"],
    ["RTHIGHPOCKPOS", "Right thigh"],
    ["CPACKPOCKPOS", "Combat pack"],
    ["BPACKPOCKPOS", "Backpack"],
    ["GUNSLINGPOCKPOS", "Gun sling"],
    ["KNIFEPOCKPOS", "Knife"]
  ] as const;
  const directions = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
  const stageBackgrounds = [
    ["checker", "Dark checker"],
    ["black", "Black"],
    ["gray", "Mid grey"],
    ["white", "White"],
    ["magenta", "Magenta"],
    ["cyan", "Cyan"]
  ] as const;
  const attachmentSlots = new Set(["HELMETPOS", "VESTPOS", "LEGPOS"]);
  const camoLabels = ["Jungle", "Urban", "Desert", "Snow"];

  function defaultScenario(): ScenarioState {
    return {
      team: 0,
      soldierClass: 0,
      civilianGroup: 0,
      camo: 0,
      urbanCamo: 0,
      desertCamo: 0,
      snowCamo: 0,
      inWater: false,
      injured: false,
      bigMercAlt: false,
      bigMercBadass: false,
      secondHandUsable: true,
      secondHandLoaded: true,
      burst: false
    };
  }

  let roots: string[] = JSON.parse(localStorage.getItem("lobot-data-roots") || "[]");
  let summary: WorkspaceSummary | null = null;
  let context: PreviewContext | null = null;
  let preview: Preview | null = null;
  let selectedCharacter = 0;
  let inventory: Inventory = Object.fromEntries(slots.map(([slot]) => [slot, 0]));
  let attachments: Attachments = {};
  let attachmentChoices: Record<string, AttachmentOption[]> = {};
  let scenario = defaultScenario();
  let animation = "";
  let animationGroup = "Idle & stance";
  let direction = 2;
  let frame = 0;
  let loading = false;
  let rendering = false;
  let error = "";
  let showSetup = roots.length === 0;
  let showDiagnostics = false;
  let showUnmatched = false;
  let showScenario = false;
  let audit: Audit | null = null;
  let auditing = false;
  let playing = false;
  let timer: ReturnType<typeof setInterval> | null = null;
  let renderSequence = 0;
  let zoom = Math.round(
    Math.min(12, Math.max(1, Number(localStorage.getItem("lobot-stage-zoom")) || 4))
  );
  let stageBackground = localStorage.getItem("lobot-stage-background") || "checker";
  if (!stageBackgrounds.some(([value]) => value === stageBackground)) {
    stageBackground = "checker";
  }

  $: currentAnimation = context?.animations.find((entry) => entry.id === animation);
  $: animationGroups = [...new Set(context?.animations.map((entry) => entry.group) || [])];
  $: visibleAnimations =
    context?.animations.filter((entry) => entry.group === animationGroup) || [];
  $: frameCount = currentAnimation?.framesPerDirection || 1;
  $: visibleLayers = preview?.layers.filter((layer) => showUnmatched || layer.status !== "unmatched") || [];

  function itemsForSlot(slot: string) {
    const candidates = summary?.items.filter((item) => item.compatibleSlots.includes(slot)) || [];
    if (slot !== "CPACKPOCKPOS" && slot !== "BPACKPOCKPOS") return candidates;
    const otherSlot = slot === "CPACKPOCKPOS" ? "BPACKPOCKPOS" : "CPACKPOCKPOS";
    const otherId = inventory[otherSlot] || 0;
    if (!otherId) return candidates;
    const other = summary?.items.find((item) => item.id === otherId);
    if (!other) return candidates;
    return candidates.filter(
      (item) =>
        Boolean(item.lbeCombo) &&
        Boolean(other.lbeCombo) &&
        ((item.lbeCombo || 0) & (other.lbeCombo || 0)) !== 0
    );
  }

  function setZoom(value: number) {
    zoom = Math.round(Math.min(12, Math.max(1, value)));
    localStorage.setItem("lobot-stage-zoom", String(zoom));
  }

  function setStageBackground(value: string) {
    stageBackground = value;
    localStorage.setItem("lobot-stage-background", value);
  }

  function request(): PreviewRequest {
    return {
      characterId: selectedCharacter,
      inventory: { ...inventory },
      attachments: Object.fromEntries(
        Object.entries(attachments).map(([slot, values]) => [slot, [...values]])
      ),
      scenario: { ...scenario },
      animation,
      direction,
      frame
    };
  }

  async function chooseInstall() {
    const selected = await open({ directory: true, multiple: false, title: "Choose JA2 1.13 install" });
    if (!selected || Array.isArray(selected)) return;
    try {
      roots = await invoke<string[]>("discover_data_roots", { installPath: selected });
      showSetup = true;
      error = "";
    } catch (reason) {
      error = String(reason);
    }
  }

  async function addRoot() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Add a Data directory as the highest-priority overlay"
    });
    if (!selected || Array.isArray(selected) || roots.includes(selected)) return;
    roots = [...roots, selected];
  }

  function removeRoot(index: number) {
    roots = roots.filter((_, rootIndex) => rootIndex !== index);
  }

  function moveRoot(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= roots.length) return;
    const next = [...roots];
    [next[index], next[target]] = [next[target], next[index]];
    roots = next;
  }

  async function loadData() {
    if (!roots.length) return;
    loading = true;
    error = "";
    preview = null;
    context = null;
    try {
      if (roots.length === 1) {
        const discovered = await invoke<string[]>("discover_data_roots", {
          installPath: roots[0]
        });
        if (discovered.length > 1) roots = discovered;
      }
      summary = await invoke<WorkspaceSummary>("load_workspace", { roots });
      localStorage.setItem("lobot-data-roots", JSON.stringify(roots));
      selectedCharacter = summary.characters[0]?.id ?? 0;
      inventory = Object.fromEntries(slots.map(([slot]) => [slot, 0]));
      attachments = {};
      attachmentChoices = {};
      scenario = defaultScenario();
      audit = null;
      showSetup = false;
      await updateContext(true);
    } catch (reason) {
      error = String(reason);
    } finally {
      loading = false;
    }
  }

  function preferredAnimation(animations: Animation[]): string {
    return (
      animations.find((entry) => entry.id === "STANDING")?.id ||
      animations.find((entry) => entry.id === "WALKING")?.id ||
      animations[0]?.id ||
      ""
    );
  }

  async function updateContext(resetAnimation = false) {
    if (!summary) return;
    error = "";
    audit = null;
    const previousAnimation = context?.animations.find((entry) => entry.id === animation);
    try {
      const next = await invoke<PreviewContext>("preview_context", { request: request() });
      context = next;
      if (resetAnimation || !next.animations.some((entry) => entry.id === animation)) {
        const equivalent =
          !resetAnimation && previousAnimation
            ? next.animations.find(
                (entry) =>
                  entry.group === previousAnimation.group && entry.label === previousAnimation.label
              )
            : undefined;
        animation = equivalent?.id || preferredAnimation(next.animations);
        frame = 0;
      } else {
        const nextFrameCount =
          next.animations.find((entry) => entry.id === animation)?.framesPerDirection || 1;
        frame = Math.min(frame, Math.max(0, nextFrameCount - 1));
      }
      animationGroup =
        next.animations.find((entry) => entry.id === animation)?.group ||
        next.animations[0]?.group ||
        "";
      await render();
    } catch (reason) {
      error = String(reason);
      preview = null;
    }
  }

  async function render() {
    if (!summary || !animation) return;
    const sequence = ++renderSequence;
    rendering = true;
    try {
      const result = await invoke<Preview>("render_preview", { request: request() });
      if (sequence === renderSequence) preview = result;
    } catch (reason) {
      if (sequence === renderSequence) error = String(reason);
    } finally {
      if (sequence === renderSequence) rendering = false;
    }
  }

  async function loadAttachmentOptions(slot: string, hostId: number) {
    if (!attachmentSlots.has(slot) || !hostId) {
      attachmentChoices = { ...attachmentChoices, [slot]: [] };
      return;
    }
    try {
      const options = await invoke<AttachmentOption[]>("attachment_options", { hostId });
      attachmentChoices = { ...attachmentChoices, [slot]: options };
    } catch (reason) {
      error = String(reason);
    }
  }

  async function selectInventory(slot: string, itemId: number) {
    inventory = { ...inventory, [slot]: itemId };
    if (attachmentSlots.has(slot)) {
      attachments = { ...attachments, [slot]: [0, 0, 0, 0] };
      await loadAttachmentOptions(slot, itemId);
    }
    await updateContext();
  }

  async function selectAttachment(slot: string, index: number, itemId: number) {
    const values = [...(attachments[slot] || [0, 0, 0, 0])];
    values[index] = itemId;
    attachments = { ...attachments, [slot]: values };
    await updateContext();
  }

  async function updateScenario() {
    const limit = context?.camouflage.appliedLimit ?? 100;
    scenario = {
      ...scenario,
      camo: Math.min(limit, Math.max(0, scenario.camo)),
      urbanCamo: Math.min(limit, Math.max(0, scenario.urbanCamo)),
      desertCamo: Math.min(limit, Math.max(0, scenario.desertCamo)),
      snowCamo: Math.min(limit, Math.max(0, scenario.snowCamo))
    };
    await updateContext();
  }

  async function runAudit() {
    if (!summary) return;
    auditing = true;
    audit = null;
    error = "";
    try {
      audit = await invoke<Audit>("audit_workspace", { request: request() });
    } catch (reason) {
      error = String(reason);
    } finally {
      auditing = false;
    }
  }

  async function selectAnimation(value: string) {
    animation = value;
    frame = 0;
    await render();
  }

  async function selectAnimationGroup(value: string) {
    animationGroup = value;
    const first = context?.animations.find((entry) => entry.group === value);
    if (first) await selectAnimation(first.id);
  }

  async function setDirection(value: number) {
    direction = value;
    await render();
  }

  async function setFrame(value: number) {
    frame = value;
    await render();
  }

  function togglePlayback() {
    playing = !playing;
    if (!playing) {
      if (timer) clearInterval(timer);
      timer = null;
      return;
    }
    timer = setInterval(() => {
      frame = (frame + 1) % Math.max(1, frameCount);
      void render();
    }, 125);
  }

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });
</script>

<svelte:head>
  <title>LOBOT Lab</title>
</svelte:head>

<header class="app-header">
  <div class="header-actions">
    {#if summary}
      <button class="health" disabled={auditing} on:click={runAudit}>
        {auditing ? "Auditing all animations…" : "Run completeness audit"}
      </button>
      <button
        class:bad={summary.warningCount > 0}
        class="health"
        on:click={() => (showDiagnostics = !showDiagnostics)}
      >
        {summary.warningCount ? `${summary.warningCount} configuration findings` : "Configuration clean"}
      </button>
    {/if}
    <button class="quiet" on:click={() => (showSetup = !showSetup)}>Data roots</button>
  </div>
</header>

{#if showSetup}
  <section class="setup-panel">
    <div>
      <span class="eyebrow">Virtual filesystem</span>
      <h2>Load game data</h2>
      <p>Roots are resolved from low to high priority, just like the game’s directory overlays.</p>
    </div>
    <div class="roots">
      {#each roots as root, index}
        <div class="root-row">
          <span class="root-order">{index + 1}</span>
          <div>
            <strong>{root.split(/[\\/]/).pop()}</strong>
            <small>{root}</small>
          </div>
          <div class="root-actions">
            <button aria-label="Move down" disabled={index === roots.length - 1} on:click={() => moveRoot(index, 1)}>↓</button>
            <button aria-label="Move up" disabled={index === 0} on:click={() => moveRoot(index, -1)}>↑</button>
            <button aria-label="Remove" on:click={() => removeRoot(index)}>×</button>
          </div>
        </div>
      {:else}
        <div class="empty-roots">Choose an install to discover its active Data directories.</div>
      {/each}
    </div>
    <div class="setup-actions">
      <button class="quiet" on:click={chooseInstall}>Choose install</button>
      <button class="quiet" on:click={addRoot}>Add overlay</button>
      <button class="primary" disabled={!roots.length || loading} on:click={loadData}>
        {loading ? "Indexing…" : "Load workspace"}
      </button>
    </div>
  </section>
{/if}

{#if showDiagnostics && summary}
  <section class="diagnostics-panel">
    <div class="diagnostics-heading">
      <div>
        <span class="eyebrow">Load-time audit</span>
        <h2>Configuration findings</h2>
      </div>
      <button class="quiet" on:click={() => (showDiagnostics = false)}>Close</button>
    </div>
    <div class="diagnostics-list">
      {#each summary.diagnostics as diagnostic}
        <article class={diagnostic.severity}>
          <span>{diagnostic.severity}</span>
          <div>
            <strong>{diagnostic.code}</strong>
            <p>{diagnostic.message}</p>
            {#if diagnostic.source}<code>{diagnostic.source}</code>{/if}
          </div>
        </article>
      {:else}
        <div class="empty-roots">No XML, entity, reference, palette, or STI findings.</div>
      {/each}
    </div>
  </section>
{/if}

{#if audit}
  <section class="diagnostics-panel audit-panel">
    <div class="diagnostics-heading">
      <div>
        <span class="eyebrow">Character + equipment audit</span>
        <h2>
          {audit.issueCount
            ? `${audit.issueCount} completeness findings`
            : "All applicable animation layers are complete"}
        </h2>
        <small>
          {audit.animationsChecked} animations · {audit.surfacesChecked.toLocaleString()} resolved
          surfaces{audit.truncated ? " · list truncated" : ""}
        </small>
      </div>
      <button class="quiet" on:click={() => (audit = null)}>Close</button>
    </div>
    <div class="diagnostics-list">
      {#each audit.findings as finding}
        <article class={finding.severity}>
          <span>{finding.severity}</span>
          <div>
            <strong>{finding.code} · {finding.animation}</strong>
            <p>{finding.message}</p>
            {#if finding.direction !== undefined || finding.layer}
              <code>
                {finding.direction !== undefined ? `Direction ${directions[finding.direction]}` : ""}
                {finding.layer ? ` · ${finding.layer}` : ""}
              </code>
            {/if}
          </div>
        </article>
      {:else}
        <div class="empty-roots">No missing files, frames, alpha companions, or layer timing mismatches.</div>
      {/each}
    </div>
  </section>
{/if}

{#if error}
  <div class="error-banner">
    <strong>Couldn’t continue</strong>
    <span>{error}</span>
    <button on:click={() => (error = "")}>×</button>
  </div>
{/if}

{#if summary && context}
  <main class="workspace">
    <aside class="controls panel">
      <section>
        <span class="eyebrow">Test subject</span>
        <label>
          Character
          <select bind:value={selectedCharacter} on:change={() => updateContext(true)}>
            {#each summary.characters as character}
              <option value={character.id}>
                {character.id} · {character.nickname || character.name} · {character.bodyTypeName}
              </option>
            {/each}
          </select>
        </label>
        <div class="body-chip">{context.bodyType}</div>
        <div class="profile-palette" aria-label="Character profile palette">
          <span>Hair <strong>{context.profilePalette.hair || "default"}</strong></span>
          <span>Skin <strong>{context.profilePalette.skin || "default"}</strong></span>
          <span>Vest <strong>{context.profilePalette.vest || "default"}</strong></span>
          <span>Pants <strong>{context.profilePalette.pants || "default"}</strong></span>
        </div>
        <div class="camouflage-readout">
          <div>
            <span>Resolved soldier palette</span>
            <strong>{context.camouflage.palette}</strong>
          </div>
          <div class="camo-values">
            {#each camoLabels as label, index}
              <span
                title={`${context.camouflage.applied[index]} applied + ${context.camouflage.worn[index]} worn`}
              >
                {label}<strong>{context.camouflage.total[index]}%</strong>
              </span>
            {/each}
            <span>Stealth<strong>{context.camouflage.stealth}%</strong></span>
          </div>
        </div>
      </section>

      <section class="inventory">
        <div class="section-heading">
          <span class="eyebrow">Inventory state</span>
          <button
            class="text-button"
            on:click={() => {
              inventory = Object.fromEntries(slots.map(([slot]) => [slot, 0]));
              attachments = {};
              attachmentChoices = {};
              void updateContext();
            }}>Clear</button
          >
        </div>
        {#each slots as [slot, label]}
          <div class="slot-row">
            <span>{label}<small>{slot}</small></span>
            <ItemPicker
              items={itemsForSlot(slot)}
              value={inventory[slot]}
              {slot}
              onSelect={(itemId) => void selectInventory(slot, itemId)}
            />
          </div>
          {#if attachmentSlots.has(slot) && inventory[slot]}
            <div class="attachment-grid">
              {#each [0, 1, 2, 3] as attachmentIndex}
                <div class="attachment-row">
                  <span>Attachment {attachmentIndex + 1}</span>
                  <ItemPicker
                    items={attachmentChoices[slot] || []}
                    value={attachments[slot]?.[attachmentIndex] || 0}
                    slot={`${slot}-attachment-${attachmentIndex}`}
                    onSelect={(itemId) =>
                      void selectAttachment(slot, attachmentIndex, itemId)}
                  />
                </div>
              {/each}
              {#if !(attachmentChoices[slot]?.length)}
                <small class="no-attachments">No direct attachment compatibility entries for this item.</small>
              {/if}
            </div>
          {/if}
        {/each}
        <p class="hint">Profile colours form the base palette; equipped LOBOT layers and their item palettes are composited over it.</p>
      </section>

      <section class="scenario">
        <button class="scenario-toggle" on:click={() => (showScenario = !showScenario)}>
          <span>
            <span class="eyebrow">Advanced scenario</span>
            <small>Water, injury, engine filters and applied camouflage</small>
          </span>
          <strong>{showScenario ? "−" : "+"}</strong>
        </button>
        {#if showScenario}
          <div class="scenario-fields">
            <div class="scenario-grid">
              <label>
                Team
                <select bind:value={scenario.team} on:change={updateScenario}>
                  <option value={0}>Player</option>
                  <option value={1}>Enemy</option>
                  <option value={2}>Creature</option>
                  <option value={3}>Militia</option>
                  <option value={4}>Civilian</option>
                </select>
              </label>
              <label>
                Soldier class
                <select bind:value={scenario.soldierClass} on:change={updateScenario}>
                  <option value={0}>None</option>
                  <option value={1}>Administrator</option>
                  <option value={2}>Elite</option>
                  <option value={3}>Army</option>
                  <option value={4}>Green militia</option>
                  <option value={5}>Regular militia</option>
                  <option value={6}>Elite militia</option>
                </select>
              </label>
            </div>
            <label>
              Civilian group
              <input
                type="number"
                min="0"
                bind:value={scenario.civilianGroup}
                on:change={updateScenario}
              />
            </label>
            <div class="camo-inputs">
              <span>Applied camouflage · configured maximum {context.camouflage.appliedLimit}%</span>
              <label>
                Jungle
                <input type="number" min="0" max={context.camouflage.appliedLimit} bind:value={scenario.camo} on:change={updateScenario} />
              </label>
              <label>
                Urban
                <input type="number" min="0" max={context.camouflage.appliedLimit} bind:value={scenario.urbanCamo} on:change={updateScenario} />
              </label>
              <label>
                Desert
                <input type="number" min="0" max={context.camouflage.appliedLimit} bind:value={scenario.desertCamo} on:change={updateScenario} />
              </label>
              <label>
                Snow
                <input type="number" min="0" max={context.camouflage.appliedLimit} bind:value={scenario.snowCamo} on:change={updateScenario} />
              </label>
            </div>
            <div class="check-grid">
              <label><input type="checkbox" bind:checked={scenario.inWater} on:change={updateScenario} /> Mid-water</label>
              <label><input type="checkbox" bind:checked={scenario.injured} on:change={updateScenario} /> Injured</label>
              <label><input type="checkbox" bind:checked={scenario.bigMercAlt} on:change={updateScenario} /> Big-merc alternate stance</label>
              <label><input type="checkbox" bind:checked={scenario.bigMercBadass} on:change={updateScenario} /> Big-merc badass rifle hold</label>
              <label><input type="checkbox" bind:checked={scenario.secondHandUsable} on:change={updateScenario} /> Off-hand usable</label>
              <label><input type="checkbox" bind:checked={scenario.secondHandLoaded} on:change={updateScenario} /> Off-hand loaded</label>
              <label><input type="checkbox" bind:checked={scenario.burst} on:change={updateScenario} /> Burst mode</label>
            </div>
          </div>
        {/if}
      </section>
    </aside>

    <section class="stage panel">
      <div class="stage-toolbar">
        <label class="animation-group-select">
          <span>Category</span>
          <select
            value={animationGroup}
            on:change={(event) => selectAnimationGroup(event.currentTarget.value)}
          >
            {#each animationGroups as group}
              <option value={group}>{group}</option>
            {/each}
          </select>
        </label>
        <label class="animation-select">
          <span>Action <small>{context.weaponMode}</small></span>
          <select value={animation} on:change={(event) => selectAnimation(event.currentTarget.value)}>
            {#each visibleAnimations as entry}
              <option value={entry.id}>
                {entry.label} · {entry.framesPerDirection}f · {entry.layerCount} layers
              </option>
            {/each}
          </select>
        </label>
        <div class="direction-control">
          <div class="direction-heading">
            <span>Facing direction</span>
            <output>{directions[direction]}</output>
          </div>
          <input
            aria-label="Facing direction"
            type="range"
            min="0"
            max="7"
            step="1"
            value={direction}
            on:input={(event) => setDirection(Number(event.currentTarget.value))}
          />
          <div class="direction-labels" aria-hidden="true">
            {#each directions as label, index}
              <span class:active={direction === index}>{label}</span>
            {/each}
          </div>
        </div>
      </div>
      {#if currentAnimation}
        <div class="variant-strip">
          <div class="variant-info">
            <span>Resolved physical variant</span>
            <code>{currentAnimation.resolvedSurface}</code>
            <small>{currentAnimation.variant}</small>
          </div>
          <div class="view-controls">
            <div class="zoom-control" aria-label="Sprite zoom">
              <button aria-label="Zoom out" on:click={() => setZoom(zoom - 1)}>−</button>
              <input
                aria-label="Sprite zoom level"
                type="range"
                min="1"
                max="12"
                step="1"
                value={zoom}
                on:input={(event) => setZoom(Number(event.currentTarget.value))}
              />
              <output>{zoom}×</output>
              <button aria-label="Zoom in" on:click={() => setZoom(zoom + 1)}>+</button>
            </div>
            <div class="background-options" aria-label="Stage background">
              {#each stageBackgrounds as [value, label]}
                <button
                  class:active={stageBackground === value}
                  data-background={value}
                  aria-label={`${label} background`}
                  title={label}
                  on:click={() => setStageBackground(value)}
                ></button>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      <div class="canvas-wrap" data-background={stageBackground}>
        <div class="sprite-plane">
          <div class="crosshair horizontal"></div>
          <div class="crosshair vertical"></div>
          {#if preview?.pngDataUrl}
            <img
              class="sprite"
              class:rendering
              src={preview.pngDataUrl}
              alt={`LOBOT composite for ${animation}, frame ${frame}`}
              style={`width: ${preview.width * zoom}px; height: ${preview.height * zoom}px`}
            />
          {:else}
            <div class="no-preview">
              <strong>No drawable layers</strong>
              <span>Inspect the layer trace for missing mappings or assets.</span>
            </div>
          {/if}
        </div>
      </div>

      <div class="timeline">
        <button class="play" class:active={playing} on:click={togglePlayback}>{playing ? "❚❚" : "▶"}</button>
        <input
          type="range"
          min="0"
          max={Math.max(0, frameCount - 1)}
          value={frame}
          on:input={(event) => setFrame(Number(event.currentTarget.value))}
        />
        <output>{frame + 1} / {frameCount}</output>
      </div>

      {#if preview}
        <div class="engine-readout">
          <span>State: {preview.animationState}</span>
          <span>Variant: {preview.resolvedSurface}</span>
          <span>World: {directions[direction]}</span>
          <span>Sprite dir: {preview.spriteDirection}</span>
          <span>STI subimage: {preview.imageIndex}</span>
          <span>{preview.width}×{preview.height}px</span>
        </div>
      {/if}
    </section>

    <aside class="trace panel">
      <div class="trace-header">
        <div>
          <span class="eyebrow">Engine trace</span>
          <h2>Layer resolution</h2>
        </div>
        <label class="toggle">
          <input type="checkbox" bind:checked={showUnmatched} />
          all
        </label>
      </div>
      <div class="layer-list">
        {#each visibleLayers as layer}
          <article class:problem={!["rendered", "hidden", "unmatched"].includes(layer.status)}>
            <div class="layer-title">
              <span class={`status ${layer.status}`}></span>
              <strong>{layer.layer}</strong>
              <code>z{layer.zIndex}</code>
            </div>
            {#if layer.surface}
              <p>{layer.surface}</p>
              <small>
                {layer.filter || "fallback"}{layer.palette ? ` · ${layer.palette}` : ""}
                {layer.spriteDirection !== undefined ? ` · dir ${layer.spriteDirection}` : ""}
                {layer.imageIndex !== undefined ? ` · #${layer.imageIndex}` : ""}
              </small>
            {:else}
              <p class="muted">{layer.detail}</p>
            {/if}
            {#if layer.status !== "rendered" && layer.status !== "unmatched"}
              <div class="finding">{layer.status}: {layer.detail}</div>
            {/if}
          </article>
        {/each}
      </div>
      {#if preview?.diagnostics.length}
        <div class="preview-findings">
          {#each preview.diagnostics as diagnostic}
            <p><strong>{diagnostic.code}</strong> {diagnostic.message}</p>
          {/each}
        </div>
      {/if}
    </aside>
  </main>

  <footer class="summary-bar">
    <span>{summary.characters.length} characters</span>
    <span>{summary.items.length} items</span>
    <span>{summary.layers} layers</span>
    <span>{summary.surfaces.toLocaleString()} surfaces</span>
    <span>{summary.filters} filters</span>
    <span>{summary.bodyTypes} logical body types</span>
  </footer>
{:else if !showSetup}
  <div class="welcome">
    <h2>Build a character from the data up.</h2>
    <p>Load a JA2 1.13 install and any mod overlay to begin tracing LOBOT layers.</p>
    <button class="primary" on:click={() => (showSetup = true)}>Configure data roots</button>
  </div>
{/if}
