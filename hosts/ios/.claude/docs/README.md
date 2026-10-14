# Documentation Index

> **Lazy-load model** — load only the docs relevant to the current task.
> Read the routing table below, then `Read` specific files as needed.

## Routing Table

| If the task involves...                        | Load                                                      |
|------------------------------------------------|-----------------------------------------------------------|
| New VIPER module or module restructuring       | architecture/viper.md, architecture/multi-package.md       |
| Adding/moving a local SPM package              | architecture/multi-package.md                              |
| Where does this helper belong (SDK ext / feature / app) | architecture/maintainability.md (#5)                |
| Brand/whitelabel, xcconfig axis, app identity          | architecture/maintainability.md (Brand Axis)          |
| EventCenter, observer events, visitor extensions | architecture/event-center.md                             |
| Using ChainRegistry from a feature             | architecture/chain-integration.md (Using ChainRegistry…)   |
| Chat, messaging, chat extensions               | architecture/chat-extension.md                             |
| Chat attachments, HOP file transfer, bitswap   | architecture/chat-attachments.md                           |
| Message compaction, compacted batches, expansion | architecture/message-compaction.md                       |
| Tab bar, bottom bar visibility, tab safe area  | architecture/tab-bar-container.md                          |
| SSO, sign-in host, dApp authentication         | architecture/sso.md                                        |
| Products, WebView, JS bridge, HostApi          | architecture/host-api-products.md                          |
| Extrinsics, transfers, signing                 | architecture/transactions.md                               |
| Storage queries, runtime calls, SCALE          | architecture/chain-integration.md                          |
| Coinage, payments, coins                       | architecture/coinage.md                                    |
| pallet-revive, EVM addresses, contract calls, ABI | architecture/revive.md                                  |
| Durable transactions, recovery pass, completion oracle | architecture/durable-transactions.md                 |
| Statement store, off-chain messaging           | architecture/statement-store-communication.md              |
| Chain status rings, liveness, connection strip | architecture/chain-status.md                                |
| Key derivation, product accounts, ring-VRF/ECDH keys | `Packages/KeyDerivation/` + architecture/sso.md (selector & wire pins) |
| WebRTC, P2P transport, data channels           | architecture/data-transport.md                             |
| Cross-cutting design questions                 | architecture/maintainability.md                            |
| Error handling, Result types                   | code/error-handling.md                                     |
| async/await, Operations, streams, actor/lock state | code/concurrency.md                                    |
| UIKit, SwiftUI, SnapKit, layout, @Observable    | code/ui-uikit.md                                           |
| Naming, comments, logging, hygiene             | code/naming-and-hygiene.md                                 |
| ServiceCoordinator, DI, service wiring         | code/di-and-services.md                                    |
| CoreData, repositories, mappers, UserDefaults  | code/data-persistence.md                                   |
| Typed wrappers, SubstrateSdk types             | code/project-types.md                                      |
| Wireframe navigation, module transitions       | code/navigation.md                                         |
| Unit tests, integration tests                  | code/testing.md                                            |
| PolkadotUI, design tokens, theming             | code/design-system.md                                      |
| Reviewing a PR (architecture)                  | review/architecture-checklist.md                           |
| Reviewing a PR (code)                          | review/code-checklist.md                                   |

## Glossary of Load-Bearing Terms

These terms carry specific meaning in reviews and plans. Use them precisely:

| Term              | Meaning                                                                                     |
|-------------------|---------------------------------------------------------------------------------------------|
| Non-trivial       | Touches >1 VIPER layer, >1 package, or introduces a new pattern                            |
| Cross-feature     | Spans multiple Modules/ directories or requires shared package changes                      |
| Peer file         | A file that MUST accompany a change (e.g., Protocols.swift when adding a Presenter method)  |
| Mid-migration     | Subsystem where old and new patterns coexist; new code follows north-star                   |
| Inherited         | Legacy code in `polkadot-app/Inherited/` — being gradually migrated, do not extend          |
| Sparingly         | Allowed but requires justification in PR description                                        |
| Package           | Local SPM package under `Packages/`; not the same as a VIPER module                        |
| Module            | VIPER feature module under `polkadot-app/Modules/`                                         |
| ViewLayout        | SwiftUI View struct hosted in UIHostingController; lives in PolkadotUI package              |
| OutputProtocol    | InteractorOutputProtocol — callback-based (not publisher-based) Interactor→Presenter flow   |
| Feature flag      | Build variant guard (#if UNSTABLE, F_DEV, TESTNET_FEATURE) for conditional behavior         |

## Reference Material

- CLAUDE.md — project-level instructions (always loaded)
- `swiftgen-templates/viper/` — canonical VIPER module templates
- `Packages/AppDependencies/Package.swift` — external dependency declarations
