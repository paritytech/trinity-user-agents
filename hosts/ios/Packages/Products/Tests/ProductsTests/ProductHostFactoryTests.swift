import Foundation
import Testing
@testable import Products

// MARK: - Stub

private struct StubDotNsTldProvider: DotNsTldProviding {
    let tld: String?
    let resolveError: (any Error)?

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
    ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: tld, resolveError: resolveError))
}

// MARK: - Tests

struct ProductHostFactoryTests {
    // MARK: - host(rawString:)

    @Test func rawStringReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).host(rawString: "browse.dot")

        #expect(result == nil)
    }

    @Test func rawStringReturnsHostWhenTldMatches() {
        let result = makeFactory(tld: "testnet").host(rawString: "browse.testnet")

        #expect(result != nil)
    }

    // MARK: - host(label:)

    @Test func labelReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).host(label: "browse")

        #expect(result == nil)
    }

    @Test func labelSuffixesAndReturnsHost() {
        let result = makeFactory(tld: "testnet").host(label: "browse")

        #expect(result?.toDotDomain().rawValue == "browse.testnet")
    }

    // MARK: - resolveHost(label:)

    @Test func resolveHostLabelSucceedsWhenTldResolved() async throws {
        let result = try await makeFactory(tld: "dot").resolveHost(label: "browse")

        #expect(result?.toDotDomain().rawValue == "browse.dot")
    }

    @Test func resolveHostLabelPropagatesError() async {
        let factory = makeFactory(tld: nil, resolveError: DotNsContractError.contentHashNotFound)

        await #expect(throws: DotNsContractError.self) {
            _ = try await factory.resolveHost(label: "browse")
        }
    }

    // MARK: - page(navigationDestination:)

    @Test func pageNavigationDestinationReturnsNilWhenTldIsNil() {
        let result = makeFactory(tld: nil).page(navigationDestination: "https://browse.dot/onboarding")

        #expect(result == nil)
    }

    @Test func pageNavigationDestinationPreservesPage() {
        let result = makeFactory(tld: "dot").page(navigationDestination: "https://browse.dot/onboarding")

        #expect(result?.host.toDotDomain().rawValue == "browse.dot")
        #expect(result?.page == "/onboarding")
    }
}
