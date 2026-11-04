import AsyncExtensions
import Foundation
import Testing
import TrUAPIHost

@testable import polkadot_app

@Suite("Funding remote config")
struct FundingConfigTests {
    @Test("Lists the configured providers in order, without repeats")
    func parsesProviders() throws {
        let providers = FundingConfig.providers(
            in: [
                "onrampUrl": "getcash.dot",
                "providers": ["ramp.dot", "other.dot", "ramp.dot"]
            ],
            logger: nil
        )

        #expect(try providers == [ProductId("ramp.dot"), ProductId("other.dot")])
    }

    @Test("Skips entries that are not product ids and keeps the rest")
    func skipsUnusableNames() throws {
        let providers = FundingConfig.providers(
            in: ["providers": ["", 42, "ramp.dot"]],
            logger: nil
        )

        #expect(try providers == [ProductId("ramp.dot")])
    }

    @Test("No object or no providers lists none")
    func missingProvidersListsNone() {
        #expect(FundingConfig.providers(in: nil, logger: nil).isEmpty)
        #expect(FundingConfig.providers(in: ["onrampUrl": "getcash.dot"], logger: nil).isEmpty)
        #expect(FundingConfig.providers(in: ["providers": "ramp.dot"], logger: nil).isEmpty)
    }

    @Test("Configured providers come first; a bundled one they name is not listed twice")
    func mergesWithBundledProviders() throws {
        let ramp = try ProductId("ramp.dot")
        let fundtest = try ProductId("fundtest-provider.dot")
        let bundled = [
            FundingProviderEntry(productId: ramp, workerManifest: "{}", origin: .bundled),
            FundingProviderEntry(productId: fundtest, workerManifest: "{}", origin: .bundled)
        ]

        let providers = TrUAPIHostRuntimeProvider.fundingProviders(configured: [ramp], bundled: bundled)

        #expect(providers == [
            FundingProviderEntry(productId: ramp, workerManifest: nil, origin: .published),
            FundingProviderEntry(productId: fundtest, workerManifest: "{}", origin: .bundled)
        ])
    }

    @Test("A refresh is listed only when it changes the providers")
    func listsOnlyChangedRefreshes() async throws {
        let ramp = try ProductId("ramp.dot")
        let other = try ProductId("other.dot")
        let published = { (ids: [ProductId]) in
            ids.map { FundingProviderEntry(productId: $0, workerManifest: nil, origin: .published) }
        }
        let refreshes = [[ramp], [ramp], [ramp, other], [ramp, other], [ramp]]
        var listed: [[FundingProviderEntry]] = []

        try await TrUAPIHostRuntimeProvider.followRefreshes(
            of: stream(refreshes),
            listed: published([ramp]),
            bundled: []
        ) { providers in
            listed.append(providers)
            return true
        }

        #expect(listed == [published([ramp, other]), published([ramp])])
    }

    @Test("Following stops once the runtime is gone")
    func stopsWhenListingIsRefused() async throws {
        let ids = try ["a.dot", "b.dot", "c.dot"].map(ProductId.init)
        var calls = 0

        try await TrUAPIHostRuntimeProvider.followRefreshes(
            of: stream(ids.map { [$0] }),
            listed: [],
            bundled: []
        ) { _ in
            calls += 1
            return false
        }

        #expect(calls == 1)
    }

    private func stream(_ refreshes: [[ProductId]]) -> AnyAsyncSequence<RemoteAppConfig> {
        AsyncStream { continuation in
            for providers in refreshes {
                continuation.yield(config(providers: providers))
            }
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    private func config(providers: [ProductId]) -> RemoteAppConfig {
        RemoteAppConfig(
            identityBackendUrl: nil,
            ipfsGatewayUrl: nil,
            dotNsResolver: nil,
            dotNsNameRegistry: nil,
            coinageInstanceId: nil,
            accountDataStoreContract: nil,
            paymentAsset: nil,
            appSharingUrl: nil,
            fundingProviders: providers
        )
    }
}
