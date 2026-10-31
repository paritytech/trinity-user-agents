import Foundation
import StructuredConcurrency
import TrUAPIHost

// MARK: - Pages

extension ProductMessageSource {
    var isAtNewestEnd: Bool {
        droppedNewer.isEmpty
    }

    var windowCount: Int {
        pages.reduce(0) { $0 + $1.ids.count }
    }

    /// The request for the page before the window, or nil when the room has no older message.
    var olderAnchor: ChatPageAnchor? {
        if let anchor = droppedOlder.last {
            return anchor
        }

        guard hasFirstPage, !olderClosed, let next = pages.first?.next else {
            return nil
        }

        return .before(cursor: next)
    }

    func contains(_ messageId: String) -> Bool {
        pages.contains { $0.ids.contains(messageId) }
    }

    func page(until messageId: String, using load: () async -> ChatPageOutcome) async -> Bool {
        for _ in 0 ..< settings.jumpPageLimit {
            if contains(messageId) {
                return true
            }

            guard case .appended = await load() else {
                return contains(messageId)
            }
        }

        return contains(messageId)
    }

    func loadNewestPage() async {
        readiness = .loading
        emit()

        switch await ask(.before(cursor: nil)) {
        case let .page(item):
            replaceWindow(with: item)
            readiness = .ready
        case .notAvailable,
             .failed,
             .unavailable:
            readiness = .unavailable
        }

        emit()
        await storeCache()
    }

    func replaceWindow(with item: ProductChatMessagesQueryItem) {
        messages = [:]
        droppedOlder = []
        droppedNewer = []
        olderFailed = false
        newerFailed = false
        olderClosed = false

        var ids: [String] = []
        for message in item.messages where messages[message.messageId] == nil {
            messages[message.messageId] = message
            ids.append(message.messageId)
        }

        // The first page does not mark the room seen: a message of the contact that the product gives as
        // `Delivered` stays new, so the screen draws the "Unread messages" separator above it.
        pages = [Page(anchor: .before(cursor: nil), next: item.next, ids: ids)]
        hasFirstPage = true
        trimNewestMessages()
    }

    func fetchOlder(_ anchor: ChatPageAnchor) async -> ChatPageOutcome {
        switch await ask(anchor) {
        case let .page(item):
            if droppedOlder.last == anchor {
                droppedOlder.removeLast()
            }

            let ids = add(item.messages)
            pages.insert(Page(anchor: anchor, next: item.next, ids: ids), at: 0)
            dropNewerPagesIfNeeded()
            return .appended(ids.count)
        case .notAvailable:
            olderClosed = true
            return .end
        case .failed:
            olderFailed = true
            return .failed
        case .unavailable:
            olderFailed = true
            readiness = .unavailable
            return .unavailable
        }
    }

    func fetchNewer(_ anchor: ChatPageAnchor) async -> ChatPageOutcome {
        var chunk: [Page] = []
        var request = anchor

        // The newest page can gain messages while it is out of the window. The source asks older pages from
        // the newest one until a page meets the window, so no message goes missing between them.
        for _ in 0 ..< settings.jumpPageLimit {
            switch await ask(request) {
            case let .page(item):
                let page = Page(anchor: request, next: item.next, ids: item.messages.map(\.messageId))
                chunk.insert(page, at: 0)

                guard request == .before(cursor: nil), droppedNewer.count == 1 else {
                    return appendNewer(chunk, items: [item])
                }

                if meetsWindow(page, next: item.next) {
                    return appendNewer(chunk, items: [item])
                }

                request = .before(cursor: item.next)
                for message in item.messages {
                    messages[message.messageId] = message
                }
            case .notAvailable,
                 .failed:
                newerFailed = true
                return .failed
            case .unavailable:
                newerFailed = true
                readiness = .unavailable
                return .unavailable
            }
        }

        // The gap did not close. The window starts again from the newest pages.
        droppedOlder = []
        droppedNewer = []
        pages = []
        return appendNewer(chunk, items: [])
    }

    func meetsWindow(_ page: Page, next: String?) -> Bool {
        guard let newestWindowPage = pages.last else {
            return true
        }

        if next == nil || page.ids.contains(where: { newestWindowPage.ids.contains($0) }) {
            return true
        }

        if case let .before(cursor) = newestWindowPage.anchor, cursor == next {
            return true
        }

        return false
    }

    func appendNewer(_ chunk: [Page], items: [ProductChatMessagesQueryItem]) -> ChatPageOutcome {
        _ = droppedNewer.popLast()

        for item in items {
            for message in item.messages {
                messages[message.messageId] = message
            }
        }

        var added = 0
        for var page in chunk {
            page.ids = page.ids.filter { id in
                !pages.contains { $0.ids.contains(id) }
            }
            added += page.ids.count
            pages.append(page)
        }

        dropOlderPagesIfNeeded()
        return .appended(added)
    }

    /// Adds the messages of a page that are not in the window yet, and gives their ids.
    func add(_ newMessages: [ChatMessage]) -> [String] {
        newMessages.compactMap { message in
            guard !contains(message.messageId) else {
                return nil
            }

            messages[message.messageId] = message
            return message.messageId
        }
    }

    func dropNewerPagesIfNeeded() {
        while windowCount > settings.windowLimit, pages.count > 1 {
            let page = pages.removeLast()
            droppedNewer.append(page.anchor)
            page.ids.forEach { messages[$0] = nil }
        }
    }

    func dropOlderPagesIfNeeded() {
        while windowCount > settings.windowLimit, pages.count > 1 {
            let page = pages.removeFirst()
            droppedOlder.append(page.anchor)
            page.ids.forEach { messages[$0] = nil }
        }

        trimNewestMessages()
    }

    /// A single page that grew past the limit with live messages loses its oldest messages.
    ///
    /// The page keeps its cursor, so a later older page can skip the dropped messages. A room screen that
    /// stays open for hundreds of live messages is the only case.
    func trimNewestMessages() {
        guard windowCount > settings.windowLimit, var page = pages.last else {
            return
        }

        let excess = windowCount - settings.windowLimit
        let dropped = page.ids.prefix(excess)
        dropped.forEach { messages[$0] = nil }
        page.ids.removeFirst(min(excess, page.ids.count))
        pages[pages.count - 1] = page
    }

    /// Asks for one page, and asks once more after `retryDelay` when the product is not available.
    func ask(_ anchor: ChatPageAnchor) async -> PageResult {
        let request = ProductChatMessagesQueryRequest(roomId: roomId, anchor: anchor, pageSize: settings.pageSize)

        do {
            let item = try await withRetry(
                maxAttempts: 2,
                initialDelay: settings.retryDelay,
                shouldRetry: { Self.isUnavailable($0) || $0 as? ChatQueryError == .refused(.notAvailable) },
                operation: { [querying] in try await querying.messagesPage(request) }
            )
            return .page(item)
        } catch let error as ChatQueryError {
            logger.warning("Page \(anchor) of room \(roomId) failed: \(error)")

            switch error {
            case .refused(.notAvailable):
                return .notAvailable
            case .timedOut,
                 .refused,
                 .actionRefused:
                return .failed
            case .failed,
                 .noAnswer:
                return .unavailable
            }
        } catch {
            logger.warning("Page \(anchor) of room \(roomId) failed: \(error)")
            return .unavailable
        }
    }

    /// True for an error that means the worker of the product does not answer.
    static func isUnavailable(_ error: Error) -> Bool {
        guard let error = error as? ChatQueryError else {
            return !(error is CancellationError)
        }

        switch error {
        case .failed,
             .noAnswer:
            return true
        case .refused,
             .actionRefused,
             .timedOut:
            return false
        }
    }
}
