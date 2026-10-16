import Foundation
import Operation_iOS
import Testing
import ChainRegistry

@testable import polkadot_app

@Suite("W3sDsfinvkRouter")
struct W3sDsfinvkRouterTests {
    private static let topicBytes = Data(repeating: 0xAA, count: 32)
    private static let keyBytes = Data([0x02]) + Data(repeating: 0x11, count: 32)

    @MainActor
    private func makeRouter(
        merchants: Result<[String: W3sMerchant], Error>
    ) -> (W3sDsfinvkRouter, MockLauncher) {
        let launcher = MockLauncher()
        let remoteConfig = StubRemoteConfig(merchants: merchants)
        let router = W3sDsfinvkRouter(remoteConfig: remoteConfig, launcher: launcher)
        return (router, launcher)
    }

    private func makeReceipt(serial: String, txNumber: String = "1") throws -> W3sDsfinvkReceipt {
        let amount = try #require(W3sAmount.parse("9.00"))
        return W3sDsfinvkReceipt(serial: serial, transactionNumber: txNumber, amount: amount)
    }

    @Test("Recipient label uses merchant.name when set")
    @MainActor
    func happyPathWithName() async throws {
        let merchants = [
            "REG-001": W3sMerchant(topic: Self.topicBytes, key: Self.keyBytes, name: "Café Müller")
        ]
        let (router, launcher) = makeRouter(merchants: .success(merchants))
        let receipt = try makeReceipt(serial: "REG-001", txNumber: "42")

        await router.route(receipt)

        let captured = try #require(launcher.lastLaunch)
        #expect(captured.merchantKey == Self.keyBytes)
        #expect(captured.topic == Self.topicBytes)
        #expect(captured.paymentId == "REG-001/42")
        #expect(captured.amount.normalizedString == "9.00")
        #expect(captured.recipientLabel == "Café Müller")
    }

    @Test("Recipient label falls back to the cash-register serial when merchant.name is missing")
    @MainActor
    func fallbackToSerialWhenNameMissing() async throws {
        let merchants = [
            "REG-001": W3sMerchant(topic: Self.topicBytes, key: Self.keyBytes, name: nil)
        ]
        let (router, launcher) = makeRouter(merchants: .success(merchants))
        let receipt = try makeReceipt(serial: "REG-001", txNumber: "42")

        await router.route(receipt)

        let captured = try #require(launcher.lastLaunch)
        #expect(captured.recipientLabel == "REG-001")
    }

    @Test("Silently drops receipts whose serial is not in the merchant config")
    @MainActor
    func unknownSerial() async throws {
        let merchants = [
            "REG-001": W3sMerchant(topic: Self.topicBytes, key: Self.keyBytes)
        ]
        let (router, launcher) = makeRouter(merchants: .success(merchants))
        let receipt = try makeReceipt(serial: "REG-UNKNOWN")

        await router.route(receipt)

        #expect(launcher.lastLaunch == nil)
    }

    @Test("Drops receipts when the merchant config fetch fails")
    @MainActor
    func fetchError() async throws {
        let (router, launcher) = makeRouter(
            merchants: .failure(NSError(domain: "test", code: 1))
        )
        let receipt = try makeReceipt(serial: "REG-001")

        await router.route(receipt)

        #expect(launcher.lastLaunch == nil)
    }
}

// MARK: - Test doubles

private final class StubRemoteConfig: RemoteConfigManaging, @unchecked Sendable {
    private let merchants: Result<[String: W3sMerchant], Error>

    init(merchants: Result<[String: W3sMerchant], Error>) {
        self.merchants = merchants
    }

    func syncedTxExtensionVersions() -> [ChainModel.Id: UInt8] {
        [:]
    }

    func fetchRemoteConfigValues() {}

    func asyncWaitChainsForRemoteConfigValues() -> CompoundOperationWrapper<[RemoteChainModel]> {
        CompoundOperationWrapper.createWithError(NSError(domain: "unused", code: 0))
    }

    func asyncWaitXcmTransfers<T: Decodable>() -> CompoundOperationWrapper<T> {
        CompoundOperationWrapper.createWithError(NSError(domain: "unused", code: 0))
    }

    func asyncWaitXcmGeneralConfig<T: Decodable>() -> CompoundOperationWrapper<T> {
        CompoundOperationWrapper.createWithError(NSError(domain: "unused", code: 0))
    }

    func asyncWaitRemoteConfig() async throws -> RemoteAppConfig {
        RemoteAppConfig(
            identityBackendUrl: URL(string: "https://polkadot-app-stg.parity.io/"),
            ipfsGatewayUrl: nil,
            dotNsResolver: nil,
            dotNsNameRegistry: nil,
            coinageInstanceId: nil,
            fundingUrl: nil,
            offrampUrl: nil,
            accountDataStoreContract: nil,
            paymentAsset: nil,
            appSharingUrl: nil
        )
    }

    func asyncWaitW3sMerchants<T: Decodable>() -> CompoundOperationWrapper<T> {
        switch merchants {
        case let .success(map):
            guard let cast = map as? T else {
                return CompoundOperationWrapper.createWithError(NSError(domain: "type-mismatch", code: 0))
            }
            return CompoundOperationWrapper.createWithResult(cast)
        case let .failure(error):
            return CompoundOperationWrapper.createWithError(error)
        }
    }
}
