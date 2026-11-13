import Foundation
import Testing

@testable import polkadot_app

@Suite("Funding remote config")
struct FundingConfigTests {
    @Test("Lists the configured providers in the published order")
    func parsesProviders() throws {
        let providers = FundingConfig.providers(
            in: [
                "onrampUrl": "getcash.dot",
                "providers": ["ramp", "other", "ramp"]
            ],
            logger: nil
        )

        #expect(providers == ["ramp", "other", "ramp"])
    }

    @Test("Skips entries that are not names and keeps the rest")
    func skipsEntriesThatAreNotNames() {
        let providers = FundingConfig.providers(
            in: ["providers": ["", 42, "ramp"]],
            logger: nil
        )

        #expect(providers == ["ramp"])
    }

    @Test("No object or no providers lists none")
    func missingProvidersListsNone() {
        #expect(FundingConfig.providers(in: nil, logger: nil).isEmpty)
        #expect(FundingConfig.providers(in: ["onrampUrl": "getcash.dot"], logger: nil).isEmpty)
        #expect(FundingConfig.providers(in: ["providers": "ramp"], logger: nil).isEmpty)
    }

    @Test("Reads the destinations next to the providers list")
    func readsDestinationsBesideProviders() {
        let json: [String: Any] = [
            "onrampUrl": "getcash.dot",
            "offrampUrl": "",
            "providers": ["ramp"],
            "other": 42
        ]

        #expect(FundingConfig.string("onrampUrl", in: json) == "getcash.dot")
        #expect(FundingConfig.string("offrampUrl", in: json) == nil)
        #expect(FundingConfig.string("providers", in: json) == nil)
        #expect(FundingConfig.string("other", in: json) == nil)
        #expect(FundingConfig.string("onrampUrl", in: nil) == nil)
    }
}
