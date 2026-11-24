import CoreData
import Foundation
import os

protocol SeenReceiptSending {
    @discardableResult
    func scheduleSeenReceipt(for chatId: Chat.Id) -> Task<Void, Never>
}

final class SeenReceiptService {
    private static let throttleInterval: Duration = .seconds(1)

    private let storageFacade: StorageFacadeProtocol
    private let sender: ChatDirectMessageSending
    private let clock: any Clock<Duration>
    private let logger: LoggerProtocol
    private let throttleWindows = OSAllocatedUnfairLock(initialState: [Chat.Id: ThrottleWindow]())

    init(
        storageFacade: StorageFacadeProtocol,
        sender: ChatDirectMessageSending,
        clock: any Clock<Duration> = ContinuousClock(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.sender = sender
        self.clock = clock
        self.logger = logger
    }
}

extension SeenReceiptService: SeenReceiptSending {
    @discardableResult
    func scheduleSeenReceipt(for chatId: Chat.Id) -> Task<Void, Never> {
        throttleWindows.withLock { windows in
            if let window = windows[chatId] {
                windows[chatId]?.hasPendingTrigger = true
                return window.task
            }

            let task = Task {
                await runThrottleWindow(for: chatId)
            }
            windows[chatId] = ThrottleWindow(task: task, hasPendingTrigger: false)
            return task
        }
    }
}

private extension SeenReceiptService {
    func runThrottleWindow(for chatId: Chat.Id) async {
        var lastSentMark: SeenMark?

        repeat {
            lastSentMark = await sendAttempt(for: chatId, lastSentMark: lastSentMark)

            guard await (try? clock.sleep(for: Self.throttleInterval)) != nil else {
                removeThrottleWindow(for: chatId)
                return
            }
        } while takePendingTrigger(for: chatId)
    }

    func sendAttempt(for chatId: Chat.Id, lastSentMark: SeenMark?) async -> SeenMark? {
        do {
            return try await sendIfAdvanced(for: chatId, lastSentMark: lastSentMark) ?? lastSentMark
        } catch {
            logger.error("Failed to send seen receipt: \(error)")
            return lastSentMark
        }
    }

    func sendIfAdvanced(for chatId: Chat.Id, lastSentMark: SeenMark?) async throws -> SeenMark? {
        guard case let .person(accountId) = chatId else {
            return nil
        }

        let state = try await storageFacade.databaseService.performRead { context in
            try context.lastSeenMessageState(in: chatId)
        }

        guard let state else {
            return nil
        }

        let mark: SeenMark = (state.order, state.timestamp)
        let isAdvancedSinceLastSent = mark > (state.lastSentOrder, state.lastSentTimestamp)
        let isAdvancedSinceLastWindowSend = lastSentMark.map { mark > $0 } ?? true
        guard isAdvancedSinceLastSent, isAdvancedSinceLastWindowSend else {
            return nil
        }

        let content = ChatRemoteMessageContent.seen(
            .init(messageId: state.messageId, timestamp: UInt64(bitPattern: state.timestamp))
        )
        sender.sendDirectly(Chat.RemoteMessage.newMessage(with: content), to: accountId)
        return mark
    }

    func takePendingTrigger(for chatId: Chat.Id) -> Bool {
        throttleWindows.withLock { windows in
            guard windows[chatId]?.hasPendingTrigger == true else {
                windows[chatId] = nil
                return false
            }
            windows[chatId]?.hasPendingTrigger = false
            return true
        }
    }

    func removeThrottleWindow(for chatId: Chat.Id) {
        throttleWindows.withLock { $0[chatId] = nil }
    }
}

private typealias SeenMark = (order: Int64, timestamp: Int64)

private struct ThrottleWindow {
    let task: Task<Void, Never>
    var hasPendingTrigger: Bool
}

private struct LastSeenMessageState {
    let messageId: String
    let order: Int64
    let timestamp: Int64
    let lastSentOrder: Int64
    let lastSentTimestamp: Int64
}

private extension NSManagedObjectContext {
    func lastSeenMessageState(in chatId: Chat.Id) throws -> LastSeenMessageState? {
        let chat: CDChat? = try first(for: .chat(for: chatId.rawRepresentation))

        guard let chat, let contact = chat.contact, contact.chatRequest == nil, !contact.isBlocked else {
            return nil
        }

        let request = CDChatMessage.fetchRequest()
        request.predicate = NSPredicate.lastSeenMessageCandidates(in: chatId)
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
        ]
        request.fetchLimit = 1

        guard let lastSeen = try fetch(request).first, let messageId = lastSeen.messageId else {
            return nil
        }

        return LastSeenMessageState(
            messageId: messageId,
            order: lastSeen.order,
            timestamp: lastSeen.timestamp,
            lastSentOrder: chat.lastSentSeenOrder,
            lastSentTimestamp: chat.lastSentSeenTimestamp
        )
    }
}
