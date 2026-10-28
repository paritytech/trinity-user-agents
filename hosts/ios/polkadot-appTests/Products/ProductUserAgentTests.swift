import Testing
@testable import polkadot_app

struct ProductUserAgentTests {
    @Test func includesVersionBuildAndModel() {
        let name = ProductUserAgent.applicationName(version: "0.10.0", build: "44", model: "iPhone17,1")

        #expect(name == "PolkadotApp/0.10.0 (44; iPhone17,1)")
    }

    @Test func omitsMissingParts() {
        #expect(ProductUserAgent
            .applicationName(version: nil, build: nil, model: "iPhone17,1") == "PolkadotApp (iPhone17,1)")
        #expect(ProductUserAgent
            .applicationName(version: "0.10.0", build: nil, model: "arm64") == "PolkadotApp/0.10.0 (arm64)")
    }
}
