<script lang="ts">
  type PickableItem = { id: number; name: string };

  export let items: PickableItem[] = [];
  export let value = 0;
  export let slot: string;
  export let onSelect: (value: number) => void;

  let query = "";
  let open = false;
  let editing = false;
  let activeIndex = 0;
  let visibleCount = 120;
  let allMatches: PickableItem[] = [];
  let matches: PickableItem[] = [];

  $: selected = items.find((item) => item.id === value);
  $: selectedLabel = selected ? formatItem(selected) : "";
  $: if (!editing) query = selectedLabel;
  $: normalizedQuery = query.trim().toLocaleLowerCase();
  $: allMatches =
    open
      ? normalizedQuery
        ? items.filter(
            (item) =>
              String(item.id).includes(normalizedQuery) ||
              item.name.toLocaleLowerCase().includes(normalizedQuery)
          )
        : items
      : [];
  $: matches = allMatches.slice(0, visibleCount);
  $: activeIndex = Math.min(activeIndex, Math.max(0, matches.length - 1));

  function formatItem(item: PickableItem) {
    return `${item.id} · ${item.name}`;
  }

  function beginSearch() {
    editing = true;
    open = true;
    query = "";
    activeIndex = 0;
    visibleCount = 120;
  }

  function updateSearch(event: Event) {
    query = (event.currentTarget as HTMLInputElement).value;
    open = true;
    activeIndex = 0;
    visibleCount = 120;
  }

  function choose(item?: PickableItem) {
    const nextValue = item?.id ?? 0;
    query = item ? formatItem(item) : "";
    editing = false;
    open = false;
    onSelect(nextValue);
  }

  function cancelSearch() {
    query = selectedLabel;
    editing = false;
    open = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      open = true;
      const nextIndex = Math.min(activeIndex + 1, Math.max(0, allMatches.length - 1));
      if (nextIndex >= matches.length && matches.length < allMatches.length) {
        visibleCount += 120;
      }
      activeIndex = nextIndex;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      activeIndex = Math.max(0, activeIndex - 1);
    } else if (event.key === "Enter" && matches[activeIndex]) {
      event.preventDefault();
      choose(matches[activeIndex]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelSearch();
    }
  }

  function loadMore(event: Event) {
    const list = event.currentTarget as HTMLDivElement;
    if (
      matches.length < allMatches.length &&
      list.scrollTop + list.clientHeight >= list.scrollHeight - 70
    ) {
      visibleCount = Math.min(visibleCount + 120, allMatches.length);
    }
  }
</script>

<div class="item-picker">
  <div class="input-wrap">
    <input
      class="item-search"
      type="text"
      role="combobox"
      aria-label={`Search items for ${slot}`}
      aria-autocomplete="list"
      aria-expanded={open}
      aria-controls={`${slot}-results`}
      autocomplete="off"
      placeholder="Search name or ID…"
      title={selectedLabel}
      value={query}
      on:focus={beginSearch}
      on:input={updateSearch}
      on:keydown={handleKeydown}
      on:blur={cancelSearch}
    />
    {#if value}
      <button
        class="clear-item"
        type="button"
        aria-label="Clear selected item"
        title="Clear selected item"
        on:mousedown|preventDefault={() => choose()}
      >×</button>
    {/if}
  </div>

  {#if open}
    <div
      class="item-results"
      id={`${slot}-results`}
      role="listbox"
      on:scroll={loadMore}
    >
      {#each matches as item, index}
        <button
          type="button"
          role="option"
          aria-selected={item.id === value}
          class:active={index === activeIndex}
          on:mousemove={() => (activeIndex = index)}
          on:mousedown|preventDefault={() => choose(item)}
        >
          <span>{item.id}</span>
          <strong>{item.name}</strong>
        </button>
      {:else}
        <div class="search-message">
          No compatible items found.
        </div>
      {/each}
      {#if allMatches.length > matches.length}
        <div class="search-message">
          Showing {matches.length} of {allMatches.length} items—scroll for more or type to filter.
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .item-picker {
    min-width: 0;
  }

  .input-wrap {
    position: relative;
  }

  .item-search {
    width: 100%;
    height: 29px;
    padding: 0 26px 0 8px;
    border: 1px solid #3d464d;
    border-radius: 2px;
    color: #e7eaed;
    background: #131619;
    font-size: 10px;
  }

  .item-search:focus {
    outline: 1px solid var(--accent-dark);
    outline-offset: 0;
  }

  .clear-item {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 21px;
    height: 21px;
    padding: 0;
    border: 0;
    color: #8e989f;
    background: transparent;
    line-height: 1;
  }

  .clear-item:hover {
    color: #e7eaed;
    background: #2c3237;
  }

  .item-results {
    max-height: 230px;
    margin-top: 3px;
    overflow: auto;
    border: 1px solid #46515a;
    background: #15191c;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.38);
    scrollbar-color: #515b63 transparent;
    scrollbar-width: thin;
  }

  .item-results button {
    width: 100%;
    min-height: 29px;
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr);
    gap: 7px;
    align-items: center;
    padding: 5px 7px;
    border: 0;
    border-bottom: 1px solid #292f34;
    border-radius: 0;
    color: #c9d0d5;
    background: transparent;
    text-align: left;
  }

  .item-results button:last-of-type {
    border-bottom: 0;
  }

  .item-results button:hover,
  .item-results button.active {
    color: #edf2f5;
    background: #29353e;
  }

  .item-results button[aria-selected="true"] {
    color: #d7e6ef;
    background: #24313a;
  }

  .item-results span {
    color: #788690;
    font: 9px/1 ui-monospace, monospace;
    text-align: right;
  }

  .item-results strong {
    min-width: 0;
    overflow: hidden;
    font-size: 10px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .search-message {
    padding: 10px;
    color: #7e8991;
    font-size: 9px;
    line-height: 1.35;
    text-align: center;
  }
</style>
