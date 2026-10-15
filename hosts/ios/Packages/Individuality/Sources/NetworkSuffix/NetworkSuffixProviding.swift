import Foundation

/// The network's suffix that personhood product contexts are built from, e.g. `paseo`.
public protocol NetworkSuffixProviding: Sendable {
    func networkSuffix() async throws -> Data
}
