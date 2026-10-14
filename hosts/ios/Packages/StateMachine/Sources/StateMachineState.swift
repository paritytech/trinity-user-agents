import Foundation

/// A discrete state in a state machine workflow.
///
/// Each state knows whether it is terminal and can produce the next state
/// via ``transit(with:)``. Errors during transitions should be handled
/// internally by returning a terminal failed state rather than throwing.
public protocol StateMachineState {
    /// Factory used to create successor states during transitions.
    associatedtype StateFactory

    /// Persistable representation of this state (e.g., a CoreData model).
    associatedtype PersistentValue

    /// Whether this state is terminal, meaning no further transitions are possible.
    var isTerminal: Bool { get }

    /// Performs processing for this state and returns the next state.
    ///
    /// Uses the factory to create the appropriate successor state.
    /// Must not throw — errors should be caught internally and mapped
    /// to a terminal failed state via the factory.
    ///
    /// - Parameter factory: Factory providing methods to create successor states.
    /// - Returns: The next state, type-erased.
    func transit(with factory: StateFactory) async -> AnyStateMachineState<StateFactory, PersistentValue>

    /// Creates a persistable memo representing this state.
    func memo() async -> PersistentValue
}
