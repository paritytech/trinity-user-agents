import Coinage
import Foundation
import Testing

@testable import polkadot_app

/// A chat payment's export is committed before the message that carries its keys is saved. The other
/// order is the one that loses money: a saved message whose export the wallet still counts would let
/// it spend coins a peer can claim.
@Suite("Transfer send order", .serialized)
struct TransferSendOrderTests {
    private let messageId = "msg-order-1"
    private let memo = TransferMemo(entries: [Data(repeating: 0x07, count: 64)], totalValue: 10)

    @Test("the export is committed before the message carrying its keys exists")
    func commitPrecedesTheMessage() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let submitter = makeSubmitter(world: world)
        let recorder = CommitRecorder()
        let peer = try #require(world.chatManager.peer.contact?.accountId)

        try await submitter.sendTransfer(memo, to: peer, messageId: messageId) {
            try await recorder.record(messageExisted: world.transfer(messageId) != nil)
        }

        #expect(await recorder.messageExistedAtCommit == [false])
        #expect(try await world.transfer(messageId) != nil)
    }

    @Test("a refused commit leaves no message behind")
    func refusedCommitSavesNothing() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let submitter = makeSubmitter(world: world)
        let peer = try #require(world.chatManager.peer.contact?.accountId)

        await #expect(throws: CommitRefused.self) {
            try await submitter.sendTransfer(memo, to: peer, messageId: messageId) {
                throw CommitRefused()
            }
        }

        #expect(try await world.transfer(messageId) == nil)
    }

    @Test("a recipient without a chat is never committed to")
    func missingContactNeverCommits() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let submitter = makeSubmitter(world: world)
        let recorder = CommitRecorder()

        await #expect(throws: TransferSubmitterError.self) {
            try await submitter.sendTransfer(memo, to: Data(repeating: 0xEE, count: 32), messageId: messageId) {
                await recorder.record(messageExisted: false)
            }
        }

        #expect(await recorder.messageExistedAtCommit.isEmpty)
    }
}

private extension TransferSendOrderTests {
    func makeSubmitter(world: TransferStateTestWorld) -> ContactChatSubmitter {
        ContactChatSubmitter(
            chatContactsProvider: ContactsLocalStorageService(storageFacade: world.facade),
            createMessageFactory: LocalMessageCreatingOperationFactory(),
            messageStore: ChatMessageTransactionalStore(storageFacade: world.facade)
        )
    }
}

private actor CommitRecorder {
    private(set) var messageExistedAtCommit: [Bool] = []

    func record(messageExisted: Bool) {
        messageExistedAtCommit.append(messageExisted)
    }
}

private struct CommitRefused: Error {}
