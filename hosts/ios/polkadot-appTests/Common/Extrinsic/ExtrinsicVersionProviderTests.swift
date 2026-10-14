import Foundation
import Testing
import SubstrateSdk
import ChainRegistry
@testable import polkadot_app

@Suite("ExtrinsicVersionProvider")
struct ExtrinsicVersionProviderTests {
    private static let verifySignature = Extrinsic.TransactionExtensionId.verifySignature
    private static let unknownChain: ChainModel.Id =
        "0x00000000000000000000000000000000000000000000000000000000deadbeef"

    private let pipelines = StubPipelineProvider()

    private func makeProvider(versions: [ChainModel.Id: UInt8] = [:]) -> ExtrinsicVersionProvider {
        let config = MockRemoteConfigManager()
        config.txExtensionVersions = versions
        return ExtrinsicVersionProvider(
            extensionVersionProvider: ExtrinsicExtensionVersionProvider(remoteConfig: config),
            pipelineProvider: pipelines
        )
    }

    private func isV4(_ version: Extrinsic.Version) -> Bool {
        if case .V4 = version { true } else { false }
    }

    private func isV5(_ version: Extrinsic.Version, extensionVersion: UInt8) -> Bool {
        if case let .V5(value) = version { value == extensionVersion } else { false }
    }

    @Test("a general transaction on the username chain is V5 with the remote-config extension version")
    func usernameChainUnsigned() async throws {
        let chain = AppConfig.Chains.usernameChain
        let provider = makeProvider(versions: [chain: 3])

        #expect(try await isV5(provider.getExtrinsicVersion(for: chain, isSigned: false), extensionVersion: 3))
        #expect(pipelines.reads.isEmpty)
    }

    @Test("the extension version defaults to 0 when absent from remote config")
    func defaultsToZero() async throws {
        let chain = AppConfig.Chains.usernameChain

        #expect(try await isV5(makeProvider().getExtrinsicVersion(for: chain, isSigned: false), extensionVersion: 0))
    }

    @Test("a signed transaction stays V5 when the runtime pipeline verifies general signatures")
    func signedWithVerifySignature() async throws {
        let chain = AppConfig.Chains.assethubChain
        pipelines.pipeline = ["CheckNonce", Self.verifySignature]
        let provider = makeProvider(versions: [chain: 2])

        #expect(try await isV5(provider.getExtrinsicVersion(for: chain, isSigned: true), extensionVersion: 2))
        #expect(pipelines.reads == [StubPipelineProvider.Read(chainId: chain, extensionVersion: 2)])
    }

    @Test("a signed transaction falls back to V4 when the runtime pipeline cannot verify a signature")
    func signedWithoutVerifySignature() async throws {
        let chain = AppConfig.Chains.assethubChain
        pipelines.pipeline = ["CheckNonce", "ChargeAssetTxPayment"]
        let provider = makeProvider(versions: [chain: 2])

        #expect(try await isV4(provider.getExtrinsicVersion(for: chain, isSigned: true)))
        #expect(try await isV5(provider.getExtrinsicVersion(for: chain, isSigned: false), extensionVersion: 2))
    }

    @Test("a pipeline that cannot be read fails the decision instead of guessing a format")
    func pipelineReadFailure() async {
        pipelines.error = StubPipelineProvider.Error.unavailable
        let provider = makeProvider()

        await #expect(throws: StubPipelineProvider.Error.unavailable) {
            try await provider.getExtrinsicVersion(for: AppConfig.Chains.assethubChain, isSigned: true)
        }
    }

    @Test("unknown chains are V4 regardless of remote config, without reading the runtime")
    func unknownChainFallsBackToV4() async throws {
        let provider = makeProvider(versions: [Self.unknownChain: 5])

        #expect(try await isV4(provider.getExtrinsicVersion(for: Self.unknownChain, isSigned: true)))
        #expect(try await isV4(provider.getExtrinsicVersion(for: Self.unknownChain, isSigned: false)))
        #expect(pipelines.reads.isEmpty)
    }
}

private final class StubPipelineProvider: TransactionExtensionPipelineProviding, @unchecked Sendable {
    struct Read: Equatable {
        let chainId: ChainModel.Id
        let extensionVersion: UInt8
    }

    enum Error: Swift.Error, Equatable {
        case unavailable
    }

    var pipeline: [String] = []
    var error: Error?
    private(set) var reads: [Read] = []

    func transactionExtensions(for chainId: ChainId, extensionVersion: UInt8) async throws -> [String] {
        reads.append(Read(chainId: chainId, extensionVersion: extensionVersion))
        if let error { throw error }
        return pipeline
    }
}
