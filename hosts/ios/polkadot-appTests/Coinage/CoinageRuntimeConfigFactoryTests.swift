import Coinage
import CoreData
import Foundation
import NovaCrypto
import Revive
import Testing
import TrUAPIHost

@testable import polkadot_app

/// The hand-over is the only time the core learns what the app's own engine knew and the chain does
/// not. An installation left out, or a high-water mark set too low, is value the core never looks for;
/// a sent payment left out is coins the core would spend again while the payee can still claim them.
@Suite("Coinage runtime config", .serialized)
struct CoinageRuntimeConfigFactoryTests {
    private static let current = Data(repeating: 0xA1, count: 32)
    private static let previous = Data(repeating: 0xB2, count: 32)

    @Test("an app that never ran its own engine hands nothing over")
    func noInstallationsHandsNothingOver() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()

        let config = try #require(makeFactory(world: world).coinageConfig())

        #expect(config.handOver == nil)
    }

    @Test("every installation is handed over with marks past every index it allocated or scanned")
    func installationsCarryTheirHighWaterMarks() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        try await seedInstallations(world: world)

        let handOver = try #require(makeFactory(world: world).coinageConfig()?.handOver)

        #expect(
            Set(handOver.installations) == [
                AppInstallation(id: Self.current, nextCoin: 5, nextEntry: 0),
                AppInstallation(id: Self.previous, nextCoin: 7, nextEntry: 10)
            ]
        )
    }

    @Test("unsettled payments are handed over under the ids the chat follows them by")
    func paymentsAreHandedOver() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        try await seedInstallations(world: world)
        let sentSecret = try Self.coinSecret()
        let receivedSecret = try Self.coinSecret()
        let sent = try await world.saveTransfer(.outgoing, coinKeys: [sentSecret])
        let received = try await world.saveTransfer(.incoming, coinKeys: [receivedSecret])

        let handOver = try #require(makeFactory(world: world).coinageConfig()?.handOver)

        let claimBy = Date.fromChatTimestamp(received.timestamp).addingTimeInterval(CoinageConstants.claimRetryWindow)
        #expect(
            try handOver.outstanding == [
                OutstandingPayment(
                    id: CoinageOperationId.chatSend(messageId: sent.messageId).value,
                    accounts: [SNKeyFactory().createPublicKey(fromSecret: sentSecret).rawData()]
                )
            ]
        )
        #expect(
            try handOver.incoming == [
                IncomingPayment(
                    id: CoinageOperationId.chatClaim(messageId: received.messageId).value,
                    claimBy: UInt64(claimBy.timeIntervalSince1970),
                    coins: [receivedSecret]
                )
            ]
        )
    }

    @Test("a payment the chat has settled is not handed over")
    func settledPaymentsStayBehind() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        try await seedInstallations(world: world)
        let received = try await world.saveTransfer(.incoming, coinKeys: [Self.coinSecret()])
        try await world.store.updateIncoming(
            messageId: received.messageId,
            state: IncomingTransferState(status: .claimed, actualValue: 10)
        )

        let handOver = try #require(makeFactory(world: world).coinageConfig()?.handOver)

        #expect(handOver.incoming.isEmpty)
    }

    @Test("the registry contract is passed as 0x and 40 hex digits")
    func registryIsHex() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let contract = Data(repeating: 0xCD, count: 20)

        let config = try #require(makeFactory(world: world, contract: contract).coinageConfig())

        #expect(config.registryContract == "0x" + String(repeating: "cd", count: 20))
    }
}

private extension CoinageRuntimeConfigFactoryTests {
    func makeFactory(world: TransferStateTestWorld, contract: EvmAddress? = nil) -> CoinageRuntimeConfigFactory {
        CoinageRuntimeConfigFactory(
            storageFacade: world.facade,
            registry: StubRegistry(contractAddress: contract),
            logger: MockLogger()
        )
    }

    static func coinSecret() throws -> Data {
        try SNKeyFactory().createKeypair(fromSeed: Data.randomOrError(of: 32)).privateKey().rawData()
    }

    /// The current installation allocated coins 0…4; the previous one was scanned to coin 7 and
    /// voucher 2 and holds voucher 9, past its scan.
    func seedInstallations(world: TransferStateTestWorld) async throws {
        try await world.facade.databaseService.performWrite { context in
            let current = try context.insertNew(CDCurrentInstallation.self)
            current.identifier = Self.current.toHex()

            let previous = try context.insertNew(CDCoinageInstallation.self)
            previous.identifier = Self.previous.toHex()
            previous.coinScanNextIndex = 7
            previous.voucherScanNextIndex = 2

            for index in 0 ..< 5 {
                let coin = try context.insertNew(CDCoin.self)
                coin.identifier = "coin-\(index)"
                coin.installationId = Self.current.toHex()
                coin.derivationIndex = Int64(index)
            }

            let voucher = try context.insertNew(CDVoucher.self)
            voucher.identifier = "voucher-9"
            voucher.installationId = Self.previous.toHex()
            voucher.derivationIndex = 9
            voucher.allocatedAt = Date()
            voucher.readyAt = Date()
        }
    }
}

private struct StubRegistry: AccountDataStoreConfigProviding {
    let contractAddress: EvmAddress?
}
