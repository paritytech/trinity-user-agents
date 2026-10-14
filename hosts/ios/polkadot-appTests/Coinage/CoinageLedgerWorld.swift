import Coinage
import CoreData
import DurableTransactions
import Foundation
import KeyDerivation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The real CoreData stack with both stores over it — the same store the app writes through, since the
/// thing under test is the transaction, not a model of it.
///
/// Shared rather than per-suite: the handoff guard and the send-atomicity guard are two views of one
/// write path, and a second copy of this stack would be free to drift from the one the other suite
/// trusts.
struct CoinageLedgerWorld {
    let facade: UserDataStorageTestFacade
    let ledger: CoinageCoreDataLedger
    let messages: ChatMessageTransactionalStore
    let messageId = "msg-atomic-1"
    let contact = Data(repeating: 0xC0, count: 32)

    init() async throws {
        facade = UserDataStorageTestFacade()
        ledger = CoinageCoreDataLedger(storageFacade: facade)
        messages = ChatMessageTransactionalStore(storageFacade: facade)

        // The mapper resolves the message's chat and refuses one that does not exist, so the chat
        // is seeded first — a payment is always sent into an existing conversation.
        let chatId = Chat.Id.person(contact).rawRepresentation
        try await facade.databaseService.performWrite { context in
            let chat = try context.insertNew(CDChat.self)
            chat.identifier = chatId
        }

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

    func message() -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: messageId,
            chatId: .person(contact),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.new),
            timestamp: 0,
            content: .text("payment"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
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
            let request = NSFetchRequest<CDChatMessage>(entityName: "CDChatMessage")
            request.predicate = NSPredicate(
                format: "%K == %@",
                #keyPath(CDChatMessage.messageId),
                id
            )

            return try context.count(for: request) > 0
        }
    }
}
