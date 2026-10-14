import Foundation

/// Type-erased wrapper for any ``StateMachineState`` with matching associated types.
///
/// Uses closure-based erasure following the project's `AnyDataProviderRepository` pattern.
/// The ``StateMachine`` works with this type directly so concrete state types
/// don't leak into the generic infrastructure.
public struct AnyStateMachineState<Factory, Persistent> {
    public let isTerminal: Bool

    private let _transit: @Sendable (Factory) async -> AnyStateMachineState
    private let _memo: @Sendable () async -> Persistent

    public init<S: StateMachineState>(
        _ state: S
    ) where S.StateFactory == Factory, S.PersistentValue == Persistent {
        isTerminal = state.isTerminal
        _transit = { factory in await state.transit(with: factory) }
        _memo = { await state.memo() }
    }

    /// Delegates to the wrapped state's transition logic.
    public func transit(with factory: Factory) async -> AnyStateMachineState {
        await _transit(factory)
    }

    /// Delegates to the wrapped state's memo production.
    public func memo() async -> Persistent {
        await _memo()
    }
}
