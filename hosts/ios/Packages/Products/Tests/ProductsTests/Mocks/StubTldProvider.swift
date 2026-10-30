import Foundation
@testable import Products

/// Serves a fixed TLD so name canonicalisation is deterministic without a chain read.
struct StubTldProvider: DotNsTldProviding {
    var tld: String = "dot"
    var resolveError: Error?

    func currentTld() -> String? { tld }

    func resolveTld() async throws -> String {
        if let resolveError {
            throw resolveError
        }

        return tld
    }

    func refresh() {}

    func reset() {}
}

/// Stands in for a network whose TLD cannot be read.
struct FailingTldProvider: DotNsTldProviding {
    struct Unreachable: Error {}

    func currentTld() -> String? { nil }

    func resolveTld() async throws -> String { throw Unreachable() }

    func refresh() {}

    func reset() {}
}
