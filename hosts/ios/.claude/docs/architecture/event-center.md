# EventCenter

## Overview

`EventCenter` is the observer/visitor event bus shared by the app and packages. It lives in `Packages/EventCenter/` and exposes three small primitives: `EventProtocol`, `EventVisitorProtocol`, `EventCenterProtocol`. `EventCenter.shared` is the live singleton; tests create fresh instances via `EventCenter(syncQueue:)`.

Moved out of the app into its own package so packages (notably `ChainRegistry`) can emit events without depending on app-level code.

## Where Things Live

| Thing                                 | Location                                                            |
|---------------------------------------|---------------------------------------------------------------------|
| `EventProtocol`, `EventVisitorProtocol`, `EventCenterProtocol` | `Packages/EventCenter/Sources/EventCenter/`              |
| `EventCenter.shared`                  | `Packages/EventCenter/`                                             |
| Chain-related events + visitor extension | `Packages/ChainRegistry/.../EventCenter/` (`ChainSyncEvents`, `RuntimeCoderEvents`, `RuntimeSyncEvents`, `EventVisitor+ChainRegistry`) |
| App-only events                       | `polkadot-app/Common/EventCenter/Events/` (`BackupStatusChanged`, `BalanceSyncState`, `SelectedCurrencyChanged`, `SelectedUsernameChanged`) |
| App-level visitor extension           | `polkadot-app/Common/EventCenter/AppEventVisiting.swift`            |

## Convention

**An event lives where it is emitted. Its visitor extension is co-located.**

- A package that emits an event owns the `EventProtocol` struct AND the `…EventVisiting` extension protocol that adds the typed `process…` methods. Example: `ChainRegistry` defines `ChainSyncDidStart` and `ChainRegistryEventVisiting` together.
- The app target defines its own `AppEventVisiting` extension and its own event structs in `Common/EventCenter/Events/`.
- `EventVisitorProtocol` (in the EventCenter package) is just a marker — no methods. Typed dispatch happens via the visitor-extension protocols, which cast inside `accept(visitor:)`.

## Canonical Event Shape

```swift
import EventCenter

public struct ChainSyncDidComplete: EventProtocol {
    public let newOrUpdatedChains: [ChainModel]
    public let removedChains: [ChainModel]

    public init(newOrUpdatedChains: [ChainModel], removedChains: [ChainModel]) {
        self.newOrUpdatedChains = newOrUpdatedChains
        self.removedChains = removedChains
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processChainSyncDidComplete(event: self)
    }
}
```

And the visitor extension lives next to it:

```swift
public protocol ChainRegistryEventVisiting: EventVisitorProtocol {
    func processChainSyncDidStart(event: ChainSyncDidStart)
    func processChainSyncDidComplete(event: ChainSyncDidComplete)
    // …
}

public extension ChainRegistryEventVisiting {
    func processChainSyncDidStart(event _: ChainSyncDidStart) {}
    func processChainSyncDidComplete(event _: ChainSyncDidComplete) {}
    // …
}
```

Default implementations are required so observers opt into only the methods they care about.

## Rules

1. **Co-locate event + visitor protocol** — package events and their `…EventVisiting` extension protocol ship together
2. **`accept(visitor:)` casts to the visitor-extension protocol** — never to a concrete observer type
3. **Use a typed event over a `Notification` userInfo dictionary** — typed payload at the API boundary
4. **Prefer event for state changes that >1 unrelated observer cares about** — for a single caller, a delegate or callback is simpler
5. **Inject `EventCenterProtocol`, not `EventCenter.shared`** — services take it through init for testability; `EventCenter.shared` is wired in factories only
6. **Don't make events optional / nullable** — if there's no payload, define an empty struct (`public init() {}`)

## When to Add an Event vs Something Else

| Situation                                                   | Use                                  |
|-------------------------------------------------------------|--------------------------------------|
| Multiple unrelated subsystems react to a state change       | EventCenter event                    |
| One module needs a callback from a service it owns          | Delegate or `InteractorOutputProtocol` |
| Reactive stream of values (price, balance)                  | `AsyncStream` / `AsyncPassthroughSubject` |
| One-shot async response                                     | `async`/`await` return value         |
