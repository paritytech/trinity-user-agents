# dashboard

Generic host feature for the home dashboard: grid layout, cards, the favourites folder, and the Add Widget / Add to Dashboard (AtD) modals. **The dashboard knows nothing about any specific content** (products, chat, …). The `dashboard-layout` domain stores cards with an **opaque `ContentCardPayload`** (`{ kind: string; … }`) and knows only one structural kind: `folder`. Every content type is injected by an owning feature through the DI seams below.

Reference integrations: `src/features/product-dashboard/` (chain products — dynamically listed) and `src/features/chat/` (a native addable — statically listed).

## How content is added: the `dashboardCardSDK` seams

A content provider registers in its own `feature.tsx` via `dashboardCardSDK(feature, { … })` plus a couple of standalone identifiers. Every handler claims only its own `kind`/ids and returns `null` otherwise, so the dashboard resolves the first matching provider.

| Seam                  | Purpose                                                                                                                                                                                   |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content` (required)  | render the card body (usually wrapped in `DashboardCardChrome`)                                                                                                                           |
| `metadata`            | card-chrome topbar icon/label, keyed on payload                                                                                                                                           |
| `actions`             | optional card topbar actions (e.g. fullscreen)                                                                                                                                            |
| `addable`             | contribute **static** `AddableDashboardCard` entries to the Add Widget catalog (native subjects like chat)                                                                                |
| `add`                 | `dashboardCardAddTransformer` — perform the add for a `DashboardAddRequest` (build the card + persist; products commit first). Drives the uniform add dispatch (`useAddDashboardContent`) |
| `open`                | `openFavoriteItemSideEffect` — open a favourites-folder item by id                                                                                                                        |
| `folderItemContent`   | render a favourites-folder item's icon + label by id                                                                                                                                      |
| `addWidgetPanel`      | render the Add Widget detail panel for a selected entry                                                                                                                                   |
| `addToDashboardModal` | render the AtD dialog's modal content for a target id                                                                                                                                     |

The `dashboardCardSDK` lists **all** dashboard seams; a provider may register each either through the SDK bundle (`dashboardCardSDK(feature, { … })`) or via a direct `feature.inject(identifier, fn)` — both wire the same DI identifier and are equivalent. The dashboard host and the reference content features mix both styles.

Standalone state seams (not DI identifiers, imported from `@/features/dashboard`):

- `publishAddWidgetCatalogSource` / `clearAddWidgetCatalogSource` (RxState) — contribute **dynamically-listed** catalog entries (e.g. products fetched from chain), which aren't static `addable` entries.

## Two content shapes

- **Static (native) content** — a fixed `AddableDashboardCard` (kind `native:*`) contributed via `addable`. The `DashboardAddRequest` carries the `entry`. Reference: chat (`chatAddableEntry`).
- **Dynamic content** — listed at runtime (e.g. published products from chain). Contribute catalog entries via `publishAddWidgetCatalogSource` from a binding component; the `DashboardAddRequest` carries the resolved value (e.g. a `Product`). Reference: product-dashboard.

A content item can be on the dashboard **as a card** and **as a favourites icon** at the same time (the `gridId` is shared; removal is independent). Set `supportsFavorites: true` on the addable entry to surface the "Add to Favorites" affordance.

## Registering handlers: prefer plain static handlers

**Most seams take a plain function registered statically** (via `dashboardCardSDK(feature, { … })` or `feature.inject(identifier, fn)` at feature-definition time) — no React, no binding component. This holds even when the handler **returns** a hook-using component: `content`, `folderItemContent`, `addWidgetPanel`, and `addToDashboardModal` register a plain function that returns a small resolver component (e.g. `ProductAtDModalResolver`, `ProductFolderItemContent`) which calls its hooks at render. Likewise `add` is plain — it calls domain/aggregate use cases (`cardsUseCase.addCardToLayout`, `foldersUseCase.addToFavorites`, `productManagementUseCase.addProductToDashboard`) directly and returns their `Promise`; do **not** wrap it in `useAddCard`/`useAction` or a binding component (see `chat`/`product-dashboard` `feature.tsx` `add:`).

**Only register from a binding component when the handler must close over a React-only value** — i.e. it genuinely needs a hook at registration time, not just at render:

- `open` handlers navigate (`useOpenChatTab` / `useOpenProductSurface` → `useNavigate`), so they register via `useSideEffect` in a persistent binding (`ChatFavoriteOpenBinding`, `ProductFavoriteOpenBinding`).
- The async add-widget catalog source publishes hook-derived data (`usePublishedWidgetListings`) into the `addWidgetCatalog` RxState from a binding (`ProductAddWidgetCatalogBinding`).

If you reach for a `useEffect` + `registerHandler`/`removeHandler` + `useLooseRef`, first check the handler actually needs a hook — most don't.

## Rules

- Import only **DI identifiers / public exports** from `@/features/dashboard` — never reach into another feature's components/hooks. The dependency is one-directional: content features → dashboard.
- The dashboard host stays content-free: no `@/domains/product` or aggregate runtime imports. The one intentional exception is a **type-only** `import { type Product }` in `di.tsx` for the typed product variant of `DashboardAddRequest` — a feature importing a domain _type_ is allowed, and genericising it to `unknown` would only add casts for no gain.

## Known limitation

A favourites-folder item whose id no provider claims (a stale id, e.g. an uninstalled product) renders an empty cell rather than being dropped from the grid — the host can't compute resolvability outside React without resolving each item with a hook. Resolvable items are unaffected. A future cleanup could prune unclaimed favourite ids.
