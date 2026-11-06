import Foundation
import Products

/// Scripted `ProductHostProviding`: a sync answer for `host(label:)` and an async answer for `resolveHost(label:)`.
final class StubProductHostProvider: ProductHostProviding, @unchecked Sendable {
    let syncHostResult: ProductHost?
    let asyncHostResult: Result<ProductHost?, Error>

    private(set) var resolveHostCallCount = 0
    private(set) var lastLabel: String?

    init(
        syncHostResult: ProductHost?,
        asyncHostResult: Result<ProductHost?, Error>
    ) {
        self.syncHostResult = syncHostResult
        self.asyncHostResult = asyncHostResult
    }

    func host(rawString _: String) -> ProductHost? {
        nil
    }

    func host(url _: URL) -> ProductHost? {
        nil
    }

    func host(navigationDestination _: String) -> ProductHost? {
        nil
    }

    func page(url _: URL) -> ProductPage? {
        nil
    }

    func page(navigationDestination _: String) -> ProductPage? {
        nil
    }

    func host(label: String) -> ProductHost? {
        lastLabel = label
        return syncHostResult
    }

    func resolveHost(label: String) async throws -> ProductHost? {
        resolveHostCallCount += 1
        lastLabel = label
        return try asyncHostResult.get()
    }

    func resolveHost(rawString _: String) async throws -> ProductHost? {
        nil
    }
}
