import Coinage
import Foundation
import Operation_iOS
import StructuredConcurrency

@testable import polkadot_app

/// Seeds and exercises the store through the production repositories and mappers.
struct BenchmarkSeeder {
    static let batchSize = 500

    let facade: BenchmarkStorageFacade

    var contactRepository: AnyDataProviderRepository<Chat.Contact> {
        facade.makeRepo(mapper: ChatContactMapper())
    }

    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        facade.makeRepo(mapper: ChatModelMapper())
    }

    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        facade.makeRepo(mapper: ChatMessageEntityMapper())
    }

    var coinRepository: AnyDataProviderRepository<Coin> {
        facade.makeRepo(mapper: CoinMapper())
    }

    var voucherRepository: AnyDataProviderRepository<Voucher> {
        facade.makeRepo(mapper: VoucherMapper())
    }

    /// Contacts, then chats, then messages: the message mapper requires its chat row and the chat
    /// mapper requires its contact row.
    func seedChats(count: Int, messagesPerChat: Int) async throws -> [Chat.Id] {
        let contacts = (0 ..< count).map { BenchmarkFixtures.contact(index: $0) }
        try await saveInBatches(contacts, using: contactRepository)

        let chats = contacts.map { BenchmarkFixtures.chat(for: $0) }
        try await saveInBatches(chats, using: chatRepository)

        let chatIds = contacts.map { BenchmarkFixtures.chatId(for: $0) }
        var messages: [Chat.LocalMessage] = []

        for (chatIndex, chatId) in chatIds.enumerated() {
            for item in 0 ..< messagesPerChat {
                messages.append(seedMessage(chatIndex: chatIndex, item: item, chatId: chatId, revision: 0))
            }
        }

        try await saveInBatches(messages, using: messageRepository)

        return chatIds
    }

    func seedCoins(count: Int) async throws -> [Coin] {
        let coins = (0 ..< count).map { BenchmarkFixtures.coin(index: $0) }
        try await saveInBatches(coins, using: coinRepository)
        return coins
    }

    func seedVouchers(count: Int, startIndex: Int) async throws {
        let vouchers = (startIndex ..< startIndex + count).map { BenchmarkFixtures.voucher(index: $0) }
        try await saveInBatches(vouchers, using: voucherRepository)
    }

    func insertMessage(index: Int, chatId: Chat.Id) async throws {
        let message = BenchmarkFixtures.message(
            id: "insert-\(index)",
            index: index,
            chatId: chatId,
            text: "inserted \(index)"
        )
        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()
    }

    func saveVoucher(index: Int) async throws {
        try await voucherRepository.saveOperation({ [BenchmarkFixtures.voucher(index: index)] }, { [] }).asyncExecute()
    }

    /// Each batch overwrites the first `rowsPerBatch` seeded ids of a rotating chat, so the per-row
    /// fetch in `CoreDataRepository.save(models:in:)` runs on every row.
    func upsertMessages(
        batches: Int,
        rowsPerBatch: Int,
        chatIds: [Chat.Id],
        recorder: LatencyRecorder
    ) async throws {
        for batch in 0 ..< batches {
            let chatIndex = batch % chatIds.count
            let rows = (0 ..< rowsPerBatch).map { item in
                seedMessage(chatIndex: chatIndex, item: item, chatId: chatIds[chatIndex], revision: batch + 1)
            }

            try await recorder.time {
                try await messageRepository.saveOperation({ rows }, { [] }).asyncExecute()
            }
        }
    }

    func fetchMessages(chatId: Chat.Id) async throws -> [Chat.LocalMessage] {
        let repository = ChatMessageRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: .localMessages(from: chatId))

        return try await repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    /// `tasks` concurrent readers, each fetching `fetchesPerTask` chats round-robin.
    func runReaderLoad(
        chatIds: [Chat.Id],
        tasks: Int,
        fetchesPerTask: Int,
        recorder: LatencyRecorder
    ) async throws {
        try await withThrowingTaskGroup(of: Void.self) { group in
            for task in 0 ..< tasks {
                group.addTask {
                    for fetch in 0 ..< fetchesPerTask {
                        let chatId = chatIds[(task * fetchesPerTask + fetch) % chatIds.count]
                        _ = try await recorder.time { try await fetchMessages(chatId: chatId) }
                    }
                }
            }

            try await group.waitForAll()
        }
    }

    /// One fetch every `interval` until cancelled; the "UI keeps reading while a bulk job runs" shape.
    func runPeriodicReader(chatIds: [Chat.Id], interval: Duration, recorder: LatencyRecorder) async {
        var fetch = 0

        while !Task.isCancelled {
            let chatId = chatIds[fetch % chatIds.count]
            _ = try? await recorder.time { try await fetchMessages(chatId: chatId) }
            fetch += 1
            try? await Task.sleep(for: interval)
        }
    }
}

private extension BenchmarkSeeder {
    func seedMessage(chatIndex: Int, item: Int, chatId: Chat.Id, revision: Int) -> Chat.LocalMessage {
        BenchmarkFixtures.message(
            id: BenchmarkFixtures.messageId(chatIndex: chatIndex, item: item),
            index: chatIndex * 1_000 + item,
            chatId: chatId,
            text: "message \(item) rev \(revision)"
        )
    }

    func saveInBatches<T: Identifiable>(_ models: [T], using repository: AnyDataProviderRepository<T>) async throws {
        var start = 0

        while start < models.count {
            let end = min(start + Self.batchSize, models.count)
            let batch = Array(models[start ..< end])
            try await repository.saveOperation({ batch }, { [] }).asyncExecute()
            start = end
        }
    }
}
