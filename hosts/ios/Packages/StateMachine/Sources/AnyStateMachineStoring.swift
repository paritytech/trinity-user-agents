import Foundation

/// Type-erased wrapper for any ``StateMachineStoring`` implementation.
///
/// Reduces the generic parameter count on ``StateMachine`` by hiding
/// the concrete store type behind closures.
public struct AnyStateMachineStoring<Persistent> {
    private let _load: @Sendable () async throws -> Persistent
    private let _save: @Sendable (Persistent) async throws -> Void

    public init<S: StateMachineStoring>(
        _ store: S
    ) where S.PersistentValue == Persistent {
        _load = { try await store.loadStateMemo() }
        _save = { try await store.saveStateMemo($0) }
    }

    public func loadStateMemo() async throws -> Persistent {
        try await _load()
    }

    public func saveStateMemo(_ value: Persistent) async throws {
        try await _save(value)
    }
}
