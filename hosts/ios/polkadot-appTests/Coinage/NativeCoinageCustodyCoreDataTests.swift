import Coinage
import CoreData
import DurableTransactions
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// Exercises the production CoreData transaction boundary, not a separate memo-store simulation.
struct NativeCoinageCustodyCoreDataTests {
    private enum Interruption: Error { case beforeSave, revoked }

    @Test func exactCustodySurvivesLaunchReleaseWhileOrdinaryProvisionalHandoffDoesNot() async throws {
        let facade = UserDataStorageTestFacade()
        try await seed([1, 2], facade: facade)
        let ledger = CoinageAssetLedgerCoreData(storageFacade: facade)
        let retained = custody(id: "retained", indices: [1])
        try await ledger.retainNativeTransfer(retained, authorization: {})
        try await ledger.precommitHandOff([.coin(2, key(2))], validation: { _ in })

        let restarted = CoinageAssetLedgerCoreData(storageFacade: facade)
        try await restarted.releaseUncommittedHandoffs()
        #expect(try await restarted.retainedNativeTransfer(custodyId: "retained") == retained)
        #expect(try await restarted.getHandoffKeys() == Set([key(1)]))
        await #expect(throws: NativeTransferCustodyError.alreadyRegistered) {
            try await restarted.retainNativeTransfer(retained, authorization: {})
        }
    }

    @Test func interruptionBeforeDatabaseSaveRollsBackTransactionsCustodyAndMarksTogether() async throws {
        let facade = UserDataStorageTestFacade()
        try await seed([1, 2, 3], facade: facade)
        let durable = DurableTxCoreDataRepository(storageFacade: facade, rowObservers: [CoinageTxRowObserver()])
        let ledger = CoinageAssetLedgerCoreData(storageFacade: facade)
        let transfer = registration()
        let retained = custody(id: "split", indices: [2, 3])
        await #expect(throws: Interruption.beforeSave) {
            _ = try await durable.register([transfer.durable]) { scope, ids in
                try ledger.registerAssets([transfer.assets], for: ids, custody: retained, authorization: {}, in: scope)
                throw Interruption.beforeSave
            }
        }
        #expect(try await durable.getAllEntries().isEmpty)
        #expect(try await ledger.retainedNativeTransfer(custodyId: "split") == nil)
        #expect(try await ledger.getHandoffKeys().isEmpty)

        // A complete commit holds both newly minted recipient and existing pass-through coins.
        _ = try await durable.register([transfer.durable]) { scope, ids in
            try ledger.registerAssets([transfer.assets], for: ids, custody: retained, authorization: {}, in: scope)
        }
        let restarted = CoinageAssetLedgerCoreData(storageFacade: facade)
        try await restarted.releaseUncommittedHandoffs()
        #expect(try await restarted.retainedNativeTransfer(custodyId: "split") == retained)
        #expect(try await restarted.getHandoffKeys() == Set([key(2), key(3)]))
        #expect(try await durable.getAllEntries().count == 1)
    }

    @Test func revokedSessionCannotRegisterInsideDatabaseExecutor() async throws {
        let facade = UserDataStorageTestFacade()
        try await seed([1, 2, 3], facade: facade)
        let durable = DurableTxCoreDataRepository(storageFacade: facade, rowObservers: [CoinageTxRowObserver()])
        let ledger = CoinageAssetLedgerCoreData(storageFacade: facade)
        let transfer = registration()
        let retained = custody(id: "revoked", indices: [2, 3])
        await #expect(throws: Interruption.revoked) {
            _ = try await durable.register([transfer.durable]) { scope, ids in
                try ledger.registerAssets([transfer.assets], for: ids, custody: retained,
                                          authorization: { throw Interruption.revoked }, in: scope)
            }
        }
        #expect(try await durable.getAllEntries().isEmpty)
        #expect(try await ledger.retainedNativeTransfer(custodyId: "revoked") == nil)
        #expect(try await ledger.getHandoffKeys().isEmpty)
    }

    private func key(_ index: UInt64) -> Data { Data(repeating: UInt8(index), count: 32) }

    private func custody(id: String, indices: [UInt64]) -> NativeTransferCustody {
        NativeTransferCustody(custodyId: id, entries: indices.map {
            NativeTransferCustody.Entry(coinDerivationIndex: $0, valueExponent: 0, publicKey: key($0))
        })
    }

    private func registration() -> CoinageTxRegistration {
        CoinageTxRegistration(txHash: Data(repeating: 9, count: 32),
                              checkpoint: BlockRef(number: 100, hash: Data(repeating: 100, count: 32)),
                              mortalityBlocks: 64, groupId: "native-test", inputs: [.coin(.own(1, key(1)))],
                              outputs: [.coin(2, key(2))])
    }

    private func seed(_ indices: [UInt64], facade: UserDataStorageTestFacade) async throws {
        let repo = facade.makeRepo(mapper: CoinMapper())
        let coins = indices.map { Coin(exponent: 0, derivationIndex: $0, age: nil, publicKey: key($0)) }
        try await repo.saveOperation({ coins }, { [] }).asyncExecute()
    }
}
