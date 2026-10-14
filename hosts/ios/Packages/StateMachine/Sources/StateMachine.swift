import Foundation

/// A generic state machine that drives states until a terminal state is reached.
///
/// Initialized with an initial state, a factory for creating successor states,
/// and a store for persisting state memos after each transition.
///
/// The execution loop:
/// 1. If the current state is terminal, return immediately.
/// 2. Call ``AnyStateMachineState/transit(with:)`` to advance.
/// 3. Persist the new state's memo via the store.
/// 4. Loop back to step 1.
///
/// Each transition is persisted before continuing,
/// so the machine is crash-safe: a restart resumes from the last persisted state.
public struct StateMachine<Factory, Persistent> {
    public let initialState: AnyStateMachineState<Factory, Persistent>
    public let factory: Factory
    public let store: AnyStateMachineStoring<Persistent>

    public init(
        initialState: AnyStateMachineState<Factory, Persistent>,
        factory: Factory,
        store: AnyStateMachineStoring<Persistent>
    ) {
        self.initialState = initialState
        self.factory = factory
        self.store = store
    }

    /// Runs the state machine until a terminal state is reached.
    ///
    /// - Returns: The terminal state.
    /// - Throws: If persisting a state memo fails.
    public func executeUntilTerminal() async throws -> AnyStateMachineState<Factory, Persistent> {
        var state = initialState

        while !state.isTerminal {
            state = await state.transit(with: factory)
            try await store.saveStateMemo(state.memo())
        }

        return state
    }
}
