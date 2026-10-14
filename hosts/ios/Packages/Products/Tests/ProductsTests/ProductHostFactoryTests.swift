import Foundation
import Testing
@testable import Products

// MARK: - Stub

private final class StubDotNsTldProvider: DotNsTldProviding {
    var tld: String?
    var resolveError: (any Error)?

    func currentTld() -> String? {
        tld
    }

    func resolveTld() async throws -> String {
        if let resolveError {
            throw resolveError
        }

        guard let tld else {
            throw DotNsContractError.tldNotFound
        }

        return tld
    }

    func refresh() {}

    func reset() {}
}

private func makeFactory(tld: String?, resolveError: (any Error)? = nil) -> ProductHostFactory {
    let stub = StubDotNsTldProvider()
    stub.tld = tld
    stub.resolveError = resolveError

    return ProductHostFactory(tldProvider: stub)
}

// MARK: - Tests

struct ProductHostFactoryTests {
    // MARK: - host(rawString:)

    @Test func rawStringReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).host(rawString: "browse.dot")

        #expect(result == nil)
    }

    @Test func rawStringReturnsHostWhenTldMatches() {
        let result = makeFactory(tld: "test").host(rawString: "browse.test")

        #expect(result != nil)
    }

    // MARK: - host(label:)

    @Test func labelReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).host(label: "browse")

        #expect(result == nil)
    }

    @Test func labelSuffixesAndReturnsHost() {
        let result = makeFactory(tld: "test").host(label: "browse")

        #expect(result?.toDotDomain() == "browse.test")
    }

    @Test func labelProvesSuffixingBeforeParsingFails() {
        // A bare label can never satisfy the isDotDomain check without suffixing.
        #expect(ProductHost.parse("browse", tld: "test") == nil)
        #expect(makeFactory(tld: "test").host(label: "browse") != nil)
    }

    // MARK: - resolveHost(label:)

    @Test func resolveHostLabelSucceedsWhenTldResolved() async throws {
        let result = try await makeFactory(tld: "dot").resolveHost(label: "browse")

        #expect(result?.toDotDomain() == "browse.dot")
    }

    @Test func resolveHostLabelPropagatesError() async {
        let factory = makeFactory(tld: nil, resolveError: DotNsContractError.contentHashNotFound)

        await #expect(throws: DotNsContractError.self) {
            _ = try await factory.resolveHost(label: "browse")
        }
    }

    // MARK: - host(navigationDestination:)

    @Test func navigationDestinationAcceptsValidDotDomain() {
        let result = makeFactory(tld: "dot").host(navigationDestination: "browse.dot")

        #expect(result != nil)
    }

    @Test func navigationDestinationRejectsExternalHost() {
        let result = makeFactory(tld: "dot").host(navigationDestination: "https://stg.revx.dev/editor")

        #expect(result == nil)
    }

    // MARK: - page(navigationDestination:)

    @Test func pageNavigationDestinationReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).page(navigationDestination: "https://browse.dot/onboarding")

        #expect(result == nil)
    }

    @Test func pageNavigationDestinationPreservesPage() {
        let result = makeFactory(tld: "dot").page(navigationDestination: "https://browse.dot/onboarding")

        #expect(result?.host.toDotDomain() == "browse.dot")
        #expect(result?.page == "/onboarding")
    }

    @Test func pageNavigationDestinationRejectsExternalHost() {
        let result = makeFactory(tld: "dot").page(navigationDestination: "https://stg.revx.dev/editor")

        #expect(result == nil)
    }
}
