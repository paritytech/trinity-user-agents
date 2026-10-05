# Chat & ChatExtension Architecture

> **What this doc is:** rules and seams for the chat subsystem.
> **What this doc is NOT:** a tour of the chat engine. The PR-litigated anti-patterns are the canonical learning material.

The current code implements **v1**: native `ChatExtension`s plug renderers/overlays/state into a host-owned message store. The **north star is RFC-0002 v2**: Products become the message data source; the Host renders. New code ships v1 idioms but does not entrench v1 assumptions v2 will retire.

---

## Glossary

- **`ChatExtension`** (api) — composition-based plug-in; renderers returned as nullable overrides.
- **`ChatBot`** — abstract `ChatExtension` for **1-room-per-extension** extensions. Auto-creates the room; auto-routes messages to `onTextMessage` / `onCustomMessage`. Built-in bots only (`SampleBot`).
- **Direct `ChatExtension`** — for **N-room** extensions (like `ProductChatExtension`) and **no-room** processors (like `CoinagePaymentProcessingExtension`).
- **`ChatExtensionRegistry`** — Dagger `Set<@JvmSuppressWildcards ChatExtension>`. Add an extension via `@Binds @IntoSet`.
- **`ChatEngine`** — orchestrator; **not exposed** from `feature/chats/api`. Other modules cannot reach into it.
- **Renderers** — `CustomChatMessageRenderer<T>`, `CustomChatFooterRenderer`, `CustomChatHeaderRenderer`, `CustomChatMenuRenderer`, `CustomChatOverlayRenderer`.
- **`ProductChatExtension`** — wrapper that hosts a product's JS in a hidden WebView and forwards `ChatHostCalls`. One per installed product; owns N rooms. Runs on the **native host runtime only** — the Rust TrUAPI core has no chat host yet, so chat products ignore the runtime toggle. Under RFC-0002 v2 this wrapper retires: Products become the message data source.

---

## Rules

1. **`blocking`** — A `ChatExtension` that owns N rooms is implemented as a `ChatBot` is wrong. `ChatBot` is the 1-room special case only. Use direct `ChatExtension` for N-room or no-room.
2. **`blocking`** — Overlay must not influence the screen below. `PILL_CLEARANCE`-style bottom padding driven by an overlay is forbidden.
3. **`blocking`** — Renderer state lives in a dedicated VM (or `@Singleton` state holder), **never** inside the bot/extension class.
4. **`blocking`** — `Fragment` classes do not leak through bot APIs to chat host code. Compose-only API; fragment names allowed only via `ownedFragmentClasses()`.
5. **`blocking`** — A v1 idiom that bakes in "the Host owns the message store" is forbidden — that's what v2 inverts. Treat message storage as an implementation detail of the Host, not as the contract.
6. **`blocking`** — Style parameters silently overridden by ambient `CompositionLocal`s are forbidden. If a caller passes `style: ChatMessageSurfaceStyle`, `direction`, etc., the renderer must honor it.
7. **`blocking`** — `reverseLayout = true` always in the chat feed. Paging works one direction only.
8. **`major`** — Custom item-entry animations are forbidden; use `LazyListScope.animateItem()`.
9. **`major`** — Other modules access chat capabilities only through the `ChatExtension` interface; never reach into `ChatEngine` or chat-impl types.
10. **`major`** — Overlay components are named generically (`ChatExtensionOverlay*`), not feature-named (`GamePillOverlay`).
11. **`major`** — Bots do not decide when to send their messages if a higher-level engine should drive it. Don't change business logic for a UI-only feature.
12. **`major`** — Chat-specific fields (`is_read`, etc.) are not smuggled through generic flags for other features.
13. **`major`** — Custom message renderers are global per extension (v1 limitation). Don't add per-chat hacks on top; v2 fixes the scoping.

## Seams

| Seam | What it does | When to extend it |
|---|---|---|
| `ChatExtension` interface | composition over inheritance — renderer overrides returned as nullable | New chat plug-in (1-room → `ChatBot`; N-room/no-room → direct) |
| `@Binds @IntoSet ChatExtension` | Registry auto-discovery | Wire the new extension |
| `customGlobalOverlayRenderer()` + `ownedFragmentClasses()` | Cross-screen overlay surface | New floating UI driven by an extension |
| `CustomChatMessageRenderer<T>` + stable `rendererId` | Custom message types | New on-wire message type |
| `subscribeNewMessages(contentTypes = ...)` | Background processor pattern | "Chat watches on-chain event" extensions (e.g. coinage payment) |
| `ProductChatExtension` wrapper | Products as N-room chat extensions | Don't fork — extend the wrapper |

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| `ChatExtension` with N rooms implemented as `ChatBot` | blocking | direct `ChatExtension` |
| Overlay drives screen layout (`PILL_CLEARANCE`) | blocking | one-way data: overlay reads, never writes layout |
| Bot owns renderer state (requires `ComputationalScope` hack) | blocking | move state to a `*StateHolder` + dedicated VM |
| Fragment class plumbed through bot API | blocking | Compose-only; use `ownedFragmentClasses()` for names |
| Style param silently overridden by ambient `CompositionLocal` | blocking | route via `ChatConfig` / explicit param |
| `reverseLayout = false` in chat feed | blocking | always `true` |
| `synthesizeSnapshot()` workaround for state outside service lifetime | blocking | snapshot is a global state holder regardless of service running |
| Custom message-entry animation | major | `LazyListScope.animateItem()` |
| Always-on `Modifier.blur` without API 31+ guard | major | guard or polyfill |
| Reaching into `ChatEngine` from another module | major | go through `ChatExtension` interface |
| Feature-named overlay component | major | rename `ChatExtensionOverlay*` |
| `is_read` / chat-specific fields abused in other features | major | dedicated entity (e.g. reactions get their own) |

## North star — RFC-0002 v2

- **Product is the data source** (messages, participants, unread counts, message actions, toolbar, footer — all product-registered handlers).
- **Host renders.** Host does NOT own the message store unless product declines a source.
- **Per-room handler scope.** Not per-product, per-extension.
- **JAM codec** for transport.

Probe questions on every chat PR:
- Does this assume the Host owns the message store?
- Does this hard-code that custom message rendering is per-extension global?
- Does this introduce a room shape that won't translate to "room scoped to a Product"?
- Does this add a footer / toolbar / action that won't easily become a per-room handler in v2?

A "yes" isn't automatically blocking, but must be named in the architect plan with a recommendation.

## Canonical examples

- 1-room bot: `SampleBot`.
- N-room product-driven: `ProductChatExtension`.
- No-room background processor: `CoinagePaymentProcessingExtension`. **This is the template for "chat watches on-chain event".**
- Overlay done right: anywhere that uses `customGlobalOverlayRenderer()` + dedicated `*OverlayViewModel` + `ownedFragmentClasses()` suppression.
- Wiring: `ChatBotsModule` `@Binds @IntoSet`.

## Where new things live

| Concept | Goes in |
|---|---|
| New extension (bot or direct) | `feature/<X>/impl/.../domain/extension/<X>Extension.kt` |
| Renderer | `feature/<X>/impl/.../presentation/chat/renderer/` (with its own VM in a sibling package) |
| Custom message content type | `feature/<X>/impl/.../data/scale/<X>MessageContent.kt` (`@Serializable`) |
| Extension registry binding | the feature's `di/` module, `@Binds @IntoSet` |
