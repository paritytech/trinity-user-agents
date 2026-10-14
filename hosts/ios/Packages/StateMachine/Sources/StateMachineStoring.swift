import Foundation

/// Persistence layer for state machine memos.
///
/// Implementations back memo storage (e.g., CoreData, UserDefaults)
/// and provide restore capability for crash-safe state recovery.
public protocol StateMachineStoring<PersistentValue> {
    associatedtype PersistentValue

    /// Loads the most recently persisted memo.
    func loadStateMemo() async throws -> PersistentValue

    /// Persists the given memo, replacing any previously stored one.
    func saveStateMemo(_ value: PersistentValue) async throws
}
