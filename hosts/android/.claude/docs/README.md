# PolkadotApp Agent Docs

This tree is **lazy-loaded** by the `/architect`, `/implementer`, and `/reviewer` skills. Files here are not auto-included in the context — load them on demand based on the task at hand.

## Layout

```
.claude/docs/
├── architecture/        ← high-level design, seams, north-star direction
│   ├── multi-module.md
│   ├── layered.md
│   ├── chat-extension.md
│   ├── host-api-products.md
│   ├── transactions.md
│   ├── chain-integration.md
│   ├── coinage.md
│   ├── statement-store-communication.md
│   ├── data-transport.md
│   └── maintainability.md
├── code/                ← concrete coding patterns and ✗/✓ examples
│   ├── results-and-errors.md
│   ├── state-management.md
│   ├── ui-compose.md
│   ├── naming-and-hygiene.md
│   ├── di-and-lifecycle.md
│   ├── database-and-scale.md
│   ├── project-types-and-units.md
│   ├── navigation-and-routers.md
│   ├── workers-and-background-sync.md
│   ├── diagnostics-and-stall-reporting.md
│   ├── testing.md
│   ├── flow-operators-reference.md      ← reference; load only when needed
│   └── nova-widget-inventory.md         ← reference; load only when needed
└── review/              ← checklists for the reviewer skill
    ├── architecture-checklist.md
    └── code-checklist.md
```

## When to load what

| Question / signal | Load |
|---|---|
| "Where does this class live?" / "Can `common` know about this?" | `architecture/multi-module.md`, `architecture/layered.md` |
| Anything mentioning chat, bots, overlays, message renderers | `architecture/chat-extension.md` |
| Products, JS bridge, HostApi handlers, WebView lifecycle | `architecture/host-api-products.md` |
| Custom origins, extrinsic submission, signed extensions | `architecture/transactions.md` |
| Storage reads, runtime APIs, SCALE codec | `architecture/chain-integration.md` |
| Coins, denominations, transfer planner, RFC-0006 payments | `architecture/coinage.md` |
| Statement-store messaging, CommunicationSession, SSO / chat rendezvous | `architecture/statement-store-communication.md` |
| WebRTC, peer channels, DataTransport, calls/games P2P | `architecture/data-transport.md` |
| "Is this proposal clean?" / boundaries / SRP | `architecture/maintainability.md` |
| Writing a ViewModel, derived flow, single-state pattern | `code/state-management.md` |
| Using `Result<T>`, handling errors, logging failures | `code/results-and-errors.md` |
| Writing a Compose screen or widget | `code/ui-compose.md` |
| Naming a class/method, deciding on defaults, splitting files | `code/naming-and-hygiene.md` |
| Dagger module, scopes, app init, services | `code/di-and-lifecycle.md` |
| Adding a Room entity/migration or SCALE conformance test | `code/database-and-scale.md` |
| Choosing a type for time / size / binary / identifier | `code/project-types-and-units.md` |
| Adding a screen / sheet navigation entry; Router/Navigator pair | `code/navigation-and-routers.md` |
| WorkManager / CoroutineWorker / stateful background jobs | `code/workers-and-background-sync.md` |
| A one-shot user-initiated operation that can stall (submission, chain read, handshake, cross-chain wait) | `code/diagnostics-and-stall-reporting.md` |
| Writing a unit test (Mockito + helpers + `with*`/`verify*` style) | `code/testing.md` |
| Discovering an unusual Flow operator | `code/flow-operators-reference.md` (reference; on demand) |
| Discovering whether a Nova widget exists for X | `code/nova-widget-inventory.md` (reference; on demand) |
| Reviewing a PR/diff | both files in `review/` plus matching docs above |

## Hard rule

When a skill (architect, implementer, reviewer) cites a rule, it must cite the **doc and section**. If a behavior isn't covered by a doc, the rule doesn't exist yet — raise it to the user instead of inventing one.

## Glossary — load-bearing terms

These terms recur as thresholds throughout the docs. Use these definitions; don't reinterpret.

- **Non-trivial** — at least one of: changes the public API of 2+ modules, introduces a new seam (interface, registry, extension point), adds ≥30 lines of domain logic that warrants tests, or could break a currently-correct user/caller. **Trivial** is the negation.
- **Cross-feature** — across `feature/<X>/` and `feature/<Y>/` *module* boundaries. **Feature-private** is anything inside a single `feature/<X>/` (`api` + `impl`). Across screens within the same feature is **feature-private**, not cross-feature — use an interactor, not a UseCase.
- **Peer file** — a file that mirrors or supports a file already listed in `PLAN.md § files_touched`. The exact enumeration lives in `.claude/skills/implementer.md § Peer files` and is what the reviewer uses to judge scope creep. Out-of-plan files that are not peers are scope creep.
- **Worst-case main-path code** — code that runs on the UI thread, inside a ViewModel, in a UI mapper, or in any synchronous response to a user action. `getOrThrow()` in this context is `blocking` (see `code/results-and-errors.md`).
- **Sparingly** (used in `architecture/multi-module.md`) — once or twice per `api` module, with a comment justifying the dependency. More than that is a smell; consider extraction.
- **Mid-migration** — the codebase declares a north-star direction for a subsystem and the current code hasn't reached it. Today this applies to: chat (RFC-0002), payments (RFC-0006), allowance (RFC-0010), host API (RFC-0020), coinage (split-capable unload). Outside these named areas, the codebase is **not** mid-migration; design as if today's shape is permanent.

## Reference material

The "north star" for chat:
**RFC-0002 Chat Extension v2** — https://raw.githubusercontent.com/paritytech/host-rust-core/517e9e23e193f2341dfaadac2f0626f7eb1d1436/docs/rfcs/0002-chat-extension-v2.md
Local notes on the direction live in `architecture/chat-extension.md` (§ "North star").

Canonical positive examples (cited throughout docs):
- ViewModel: `feature/sso/impl/.../presentation/pairRequest/PairRequestViewModel.kt`
- Extrinsic submission: `feature/transactions/impl/.../data/RealExtrinsicService.kt`
- ChatExtension: `feature/chats/impl/.../domain/payment/CoinagePaymentProcessingExtension.kt` (lightweight) and `feature/chats/impl/.../domain/middleware/bot/sample/SampleBot.kt` (bot)
- Mixin / component composition: `code/state-management.md § Mixins`.
