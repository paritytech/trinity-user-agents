import AsyncExtensions
import BigInt
import CoreData
import Foundation
import FoundationExt
import Operation_iOS
import SubstrateSdk

@testable import polkadot_app

/// A person chat on an in-memory user store with a transfer state store and a message snapshot,
/// so store and monitor tests exercise the real mapper, predicates and touch path.
final class TransferStateTestWorld {
    enum Direction {
        case incoming
        case outgoing
    }

    let facade = UserDataStorageTestFacade()
    let chatManager: TestChatManager
    let store: TransferStateCoreDataStore
    let messageProviderFactory: ChatMessageDataProviderFactory
    private let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>

    init(dateProvider: any DateProviding = NowDateProvider()) {
        chatManager = TestChatManager(peer: MockChatPeer.person(), facade: facade)
        store = TransferStateCoreDataStore(storageFacade: facade, dateProvider: dateProvider)
        messageRepository = ChatMessageRepositoryFactory(storageFacade: facade).createRepository(forFilter: nil)
        messageProviderFactory = ChatMessageDataProviderFactory(
            repositoryFactory: ChatMessageRepositoryFactory(storageFacade: facade),
            operationQueue: OperationQueue(),
            logger: Logger.shared
        )
    }

    func setup() async throws {
        try await chatManager.setup()
    }

    @discardableResult
    func saveTransfer(
        _ direction: Direction,
        totalValue: Balance = 10,
        coinKeys: [Data] = [Data(repeating: 0x07, count: 32)]
    ) async throws -> Chat.LocalMessage {
        let content = Chat.LocalMessage.Content.Transfer(totalValue: totalValue, coinKeys: coinKeys)
        let status: Chat.LocalMessage.Status =
            switch direction {
            case .incoming: .incoming(.new)
            case .outgoing: .outgoing(.sent)
            }
        return try await chatManager.sendMessage(.coinageSend(content), status: status)
    }

    func transfer(_ messageId: Chat.MessageId) async throws -> Chat.LocalMessage.Content.Transfer? {
        let message = try await messageRepository
            .fetchOperation(by: { messageId }, options: RepositoryFetchOptions())
            .asyncExecute()
        guard case let .coinageSend(transfer)? = message?.content else { return nil }
        return transfer
    }

    func deleteMessage(_ messageId: Chat.MessageId) async throws {
        try await messageRepository.saveOperation({ [] }, { [messageId] }).asyncExecute()
    }

    /// The transfer state of `messageId` on every chat snapshot, in order.
    func stateStream(of messageId: Chat.MessageId) -> AnyAsyncSequence<Chat.LocalMessage.Content.Transfer.State?> {
        messageProviderFactory
            .subscribeChatMessages(chatManager.chatId)
            .compactMap { messages -> Chat.LocalMessage.Content.Transfer.State?? in
                guard let message = messages.first(where: { $0.messageId == messageId }),
                      case let .coinageSend(transfer) = message.content
                else { return nil }
                return .some(transfer.state)
            }
            .eraseToAnyAsyncSequence()
    }

    func incomingRowCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: NSFetchRequest<CDIncomingTransferState>(entityName: "CDIncomingTransferState"))
        }
    }
}

extension AnyAsyncSequence where Element: Equatable {
    /// Elements up to and including the first one satisfying `predicate`.
    func collect(until predicate: @escaping (Element) -> Bool) async throws -> [Element] {
        var collected: [Element] = []
        for try await element in self {
            collected.append(element)
            if predicate(element) { break }
        }
        return collected
    }
}
