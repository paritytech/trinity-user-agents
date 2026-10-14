import Foundation

/// Consumers that only need to retain connections (e.g. background executors) depend on
/// this instead of the full ``ChainRegistryProtocol``.
public protocol ConnectionRetentionProviding: Sendable {
    func retainConnections(_ scope: ConnectionRetainScope) -> ConnectionRetainToken
}
