<script lang="ts">
  import { errorPopupState, pageLayoutState } from "$lib/stores.svelte"
  import { Layout, PageLockType, PagePane } from "$lib/types"
  import { superForm } from "sveltekit-superforms"
  import { untrack } from "svelte"

  import type { PageProps } from "./$types"

  let { pagePaneState = $bindable(), data }: PageProps & { pagePaneState: PagePane } =
    $props()

  const { form, enhance } = superForm(
    untrack(() => data.forms.pageLockForm),
    {
      dataType: "json",
      onSubmit: async ({ jsonData }) => {
        jsonData({
          ...$form,
          expiresAt: $form.expiresAt ? new Date($form.expiresAt).toISOString() : undefined
        })
      },
      onResult: async ({ result }) => {
        if (result.type === "success") {
          pagePaneState = PagePane.None
        }
        if (result.type === "failure" && result.data) {
          errorPopupState.current = {
            state: true,
            message: result.data.message,
            data: result.data.data
          }
        }
      }
    }
  )

  $form.lockType = PageLockType.PermissionOnly
  $form.reason = ""
  $form.overrideExisting = false

</script>

{#if pageLayoutState.current === Layout.WIKIDOT}
  <h1 class="page-lock-header">{data.internationalization?.["wiki-page-lock"]}</h1>
{:else}
  <h2 class="page-lock-header">{data.internationalization?.["wiki-page-lock"]}</h2>
{/if}

<form id="page-lock" class="page-lock" action="?/lockCreate" method="POST" use:enhance>
  <div class="page-lock-type-group">
    <div>
      <input
        id="page-lock-type-permission"
        name="lockType"
        type="radio"
        value={PageLockType.PermissionOnly}
        bind:group={$form.lockType}
      />
      <label for="page-lock-type-permission">{data.internationalization?.["wiki-page-lock.permission-only"]}</label>
    </div>
    <div>
      <input
        id="page-lock-type-author"
        name="lockType"
        type="radio"
        value={PageLockType.AuthorOnly}
        bind:group={$form.lockType}
      />
      <label for="page-lock-type-author">{data.internationalization?.["wiki-page-lock.author-only"]}</label>
    </div>
  </div>

  <textarea
    name="reason"
    class="page-lock-reason"
    placeholder={data.internationalization?.["wiki-page-lock.reason"]}
    bind:value={$form.reason}
  ></textarea>

  <label class="page-lock-expires-at" for="page-lock-expires-at">
    {data.internationalization?.["wiki-page-lock.expires-at"]}
    <input
      id="page-lock-expires-at"
      name="expiresAt"
      type="datetime-local"
      bind:value={$form.expiresAt}
    />
  </label>

  <label class="page-lock-override">
    <input
      name="overrideExisting"
      type="checkbox"
      bind:checked={$form.overrideExisting}
    />
    {data.internationalization?.["wiki-page-lock.override"]}
  </label>

  {#if pageLayoutState.current === Layout.WIKIDOT}
    <div class="buttons">
      <input
        class="btn btn-danger"
        onclick={() => (pagePaneState = PagePane.None)}
        type="button"
        value={data.internationalization?.cancel}
      />
      <input class="btn btn-primary" type="submit" value={data.internationalization?.["wiki-page-lock"]} />
    </div>
  {:else}
    <div class="action-row page-lock-actions">
      <button
        class="action-button page-lock-button button-cancel clickable"
        onclick={() => (pagePaneState = PagePane.None)}
        type="button"
      >
        {data.internationalization?.cancel}
      </button>
      <button class="action-button page-lock-button button-lock clickable" type="submit">
        {data.internationalization?.["wiki-page-lock"]}
      </button>
    </div>
  {/if}
</form>

<style lang="scss">
  .page-lock {
    display: flex;
    flex-direction: column;
    gap: 15px;
    align-items: stretch;
    justify-content: stretch;
    width: 100%;
    padding: 0 0 2em;
  }

  .page-lock-type-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
