import Coinage
import CoreData
import DurableTransactions
import Foundation
import KeyDerivation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The real CoreData stack with both stores over it: the coinage ledger and the record of a chat payment, the same
/// stores the app writes through, since the thing under test is the transaction, not a model of it.
///
/// Shared rather than per-suite: the handoff guard and the send-atomicity guard are two views of one
/// write path, and a second copy of this stack would be free to drift from the one the other suite
/// trusts.
struct CoinageLedgerWorld {
    let facade: UserDataStorageTestFacade
    let ledger: CoinageCoreDataLedger
    /// The record of the payment, which carries the keys, as the room of the chat product keeps it.
    let messages: ProductChatPaymentStore
    let messageId = "msg-atomic-1"
    let contact = Data(repeating: 0xC0, count: 32)

    init() async throws {
        facade = UserDataStorageTestFacade()
        ledger = CoinageCoreDataLedger(storageFacade: facade)
        messages = ProductChatPaymentStore(storageFacade: facade)

        // The asset ledger joins each input and output to a persisted coin row.
        let repository = facade.makeRepo(mapper: CoinMapper())
        let coins = (1 ... 3).map { item in
            Coin(
                exponent: 0,
                derivationIndex: Self.keyIndex(item),
                age: nil,
                publicKey: Self.coinKey(item)
            )
        }
        try await repository.saveOperation({ coins }, { [] }).asyncExecute()
    }

    static func keyIndex(_ item: Int) -> CoinageKeyIndex {
        CoinageKeyIndex(installation: .test, item: UInt64(item))
    }

    /// Stable per index, so the persisted coins' keys match the rows the transactions name.
    static func coinKey(_ item: Int) -> PublicKey {
        withUnsafeBytes(of: UInt64(item).bigEndian) { Data($0) }
    }

    func coin(_ item: Int) -> OwnAsset {
        .coin(Self.keyIndex(item), Self.coinKey(item))
    }

    func message() -> ProductChatPayment {
        ProductChatPayment(
            messageId: messageId,
            productId: "chat.dot",
            roomId: "dm-" + contact.toHex(),
            incoming: false,
            timestamp: 0,
            totalValue: 1,
            coinKeys: [Self.coinKey(1)],
            delivery: .sending,
            state: nil
        )
    }

    func precommit(_ assets: [OwnAsset]) async throws {
        try await ledger.ledger.precommitHandOff(assets) { _ in }
    }

    /// Schedules one transaction inside `scope`, the way `PreparedTransfer.commit(in:)` does.
    func schedule(
        spending input: OwnAsset,
        minting output: OwnAsset,
        in scope: any DurableTxRegistrationScope
    ) throws -> [CoinageTxId] {
        let assets = CoinageAssetRegistration(
            inputs: [input.asInput],
            outputs: [output]
        )

        return try ledger.durable.schedule(
            [DurableTxSchedule(
                domainId: .coinage,
                groupId: messageId,
                policy: SubmissionPolicy(id: SubmissionPolicyId("test-policy"), params: Data([1]))
            )],
            joining: scope
        ) { scope, ids in
            try ledger.ledger.registerAssets([assets], for: ids, in: scope)
        }
    }

    func scheduledEntries() async throws -> [CoinageTxEntry] {
        try await ledger.getOperationGroupStatuses(messageId)
    }

    func messageExists() async throws -> Bool {
        let id = messageId

        return try await facade.databaseService.performRead { context in
            let request = NSFetchRequest<CDProductChatPayment>(entityName: "CDProductChatPayment")
            request.predicate = NSPredicate(
                format: "%K == %@",
                #keyPath(CDProductChatPayment.messageId),
                id
            )

            return try context.count(for: request) > 0
        }
    }
}

extension ProductChatPaymentStore {
    /// Keeps a payment of the user, running `onSaved` inside its transaction, as the app sends one.
    func save(
        _ payment: ProductChatPayment,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        try await saveOutgoing(payment, onSaved: onSaved)
    }
}
