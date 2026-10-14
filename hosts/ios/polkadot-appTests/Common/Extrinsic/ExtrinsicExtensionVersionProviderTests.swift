import Foundation
import Testing
import SubstrateSdk
import ChainRegistry
@testable import polkadot_app

@Suite("ExtrinsicExtensionVersionProvider")
struct ExtrinsicExtensionVersionProviderTests {
    private func makeProvider(versions: [ChainModel.Id: UInt8] = [:]) -> ExtrinsicExtensionVersionProvider {
        let config = MockRemoteConfigManager()
        config.txExtensionVersions = versions
        return ExtrinsicExtensionVersionProvider(remoteConfig: config)
    }

    private func isV4(_ version: Extrinsic.Version) -> Bool {
        if case .V4 = version { true } else { false }
    }

    private func isV5(_ version: Extrinsic.Version, extensionVersion: UInt8) -> Bool {
        if case let .V5(value) = version { value == extensionVersion } else { false }
    }

    @Test("V4 request resolves to V4 and ignores remote config")
    func v4IgnoresRemoteConfig() {
        let chain = AppConfig.Chains.usernameChain

        #expect(isV4(makeProvider(versions: [chain: 7]).getExtensionVersion(for: .V4, chainId: chain)))
    }

    @Test("V5 request reads the per-chain remote-config extension version")
    func v5ReadsRemoteConfig() {
        let chain = AppConfig.Chains.usernameChain

        #expect(isV5(
            makeProvider(versions: [chain: 7]).getExtensionVersion(for: .V5, chainId: chain),
            extensionVersion: 7
        ))
    }

    @Test("V5 request defaults to extension version 0 when absent from remote config")
    func v5DefaultsToZero() {
        let chain = AppConfig.Chains.usernameChain

        #expect(isV5(makeProvider().getExtensionVersion(for: .V5, chainId: chain), extensionVersion: 0))
    }
}
