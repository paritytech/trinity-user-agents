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

    @Test("Reads the destinations next to the providers list")
    func readsDestinationsBesideProviders() {
        let json: [String: Any] = [
            "onrampUrl": "getcash.dot",
            "offrampUrl": "",
            "providers": ["ramp.dot"],
            "other": 42
        ]

        #expect(FundingConfig.string("onrampUrl", in: json) == "getcash.dot")
        #expect(FundingConfig.string("offrampUrl", in: json) == nil)
        #expect(FundingConfig.string("providers", in: json) == nil)
        #expect(FundingConfig.string("other", in: json) == nil)
        #expect(FundingConfig.string("onrampUrl", in: nil) == nil)
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
}
