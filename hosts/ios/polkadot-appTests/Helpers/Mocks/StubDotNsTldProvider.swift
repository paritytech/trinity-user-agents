import Foundation
@testable import polkadot_app
import Products

/// Dumb DotNs TLD double: reports a fixed cached TLD (or none). `resolveTld()` mirrors the cached
/// value, throwing `DotNsTldError.unavailable` when absent; `refresh()` is a no-op.
struct StubDotNsTldProvider: DotNsTldProviding {
    let tld: String?

    func currentTld() -> String? { tld }

    func resolveTld() async throws -> String {
        guard let tld else { throw DotNsTldError.unavailable }
        return tld
    }

    func refresh() {}

    func reset() {}
}
