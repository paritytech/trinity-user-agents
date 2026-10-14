import Foundation
import Operation_iOS
import StructuredConcurrency

actor CompactedMessageExpansionContext {
    typealias ExpansionModel = CompactedExpansionMessageMapper.Model

    struct Pending {
        let messageId: String
        let onConfirm: @Sendable () -> Task<Void, Never>
    }

    private var tasks: [String: Task<Void, Never>] = [:]
    private var pendingTasks: [Pending] = []
    private let expansionRepository: AnyDataProviderRepository<ExpansionModel>
    private let logger: LoggerProtocol

    let maxConcurrentExpansions: Int

    init(
        storageFacade: StorageFacadeProtocol,
        maxConcurrentExpansions: Int = 100,
        logger: LoggerProtocol
    ) {
        expansionRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(CompactedExpansionMessageMapper())
            )
        )

        self.maxConcurrentExpansions = maxConcurrentExpansions
        self.logger = logger
    }

    func processExpansion(
        messageId: String,
        onConfirm: @escaping @Sendable () -> Task<Void, Never>
    ) {
        guard
            tasks[messageId] == nil,
            !pendingTasks.contains(where: { $0.messageId == messageId })
        else {
            return
        }

        if tasks.count < maxConcurrentExpansions {
            tasks[messageId] = onConfirm()
        } else {
            pendingTasks.append(Pending(
                messageId: messageId,
                onConfirm: onConfirm
            ))
        }
    }

    func handleCompleted(
        messageId: Chat.MessageId,
        result: ExpansionModel
    ) async throws {
        try await expansionRepository.saveOperation({ [result] }, { [] }).asyncExecute()

        tasks[messageId] = nil

        startNextPendingIfNeeded()
    }

    func handleFailed(messageId: String) {
        tasks[messageId] = nil
        startNextPendingIfNeeded()
    }

    func cancelAll() {
        tasks.values.forEach { $0.cancel() }
        tasks.removeAll()
        pendingTasks.removeAll()
    }
}

private extension CompactedMessageExpansionContext {
    func startNextPendingIfNeeded() {
        while tasks.count < maxConcurrentExpansions, !pendingTasks.isEmpty {
            let pending = pendingTasks.removeFirst()
            tasks[pending.messageId] = pending.onConfirm()
        }
    }
}
