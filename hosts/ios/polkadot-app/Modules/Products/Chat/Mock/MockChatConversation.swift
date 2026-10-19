#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    /// The messages of one room of the mock product, oldest first, as a product that keeps its messages holds them.
    ///
    /// A cursor is the id of the message at the edge of the page that the host has, so a message that arrives
    /// between two pages does not move the next page.
    struct MockChatConversation: Equatable {
        private(set) var messages: [ChatMessage]

        func message(_ messageId: String) -> ChatMessage? {
            messages.first { $0.messageId == messageId }
        }

        /// One page of `messages_query`, oldest first.
        func page(_ anchor: ChatPageAnchor, size: Int) -> ProductChatMessagesQueryItem {
            switch anchor {
            case let .before(cursor):
                let end = cursor.flatMap(index(of:)) ?? messages.endIndex
                let start = max(end - size, messages.startIndex)
                let page = Array(messages[start ..< end])
                return ProductChatMessagesQueryItem(messages: page, next: start > 0 ? page.first?.messageId : nil)
            case let .after(cursor):
                let start = cursor.flatMap(index(of:)).map { $0 + 1 } ?? messages.startIndex
                let end = min(start + size, messages.endIndex)
                let page = start < end ? Array(messages[start ..< end]) : []
                return ProductChatMessagesQueryItem(
                    messages: page,
                    next: end < messages.endIndex ? page.last?.messageId : nil
                )
            }
        }

        /// One page of `search_query`: the messages whose text holds `text`, newest first.
        func search(_ text: String, cursor: String?, size: Int) -> ProductChatSearchQueryItem {
            let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !needle.isEmpty else { return ProductChatSearchQueryItem(hits: [], next: nil) }

            let hits = messages.reversed().filter { $0.body.text?.localizedCaseInsensitiveContains(needle) == true }
            let start = cursor.flatMap { cursor in hits.firstIndex { $0.messageId == cursor }.map { $0 + 1 } } ?? 0
            let end = min(start + size, hits.count)
            let page = start < end ? Array(hits[start ..< end]) : []
            return ProductChatSearchQueryItem(hits: page, next: end < hits.count ? page.last?.messageId : nil)
        }

        mutating func append(_ message: ChatMessage) {
            messages.append(message)
        }

        /// Gives false for a message that the room does not hold.
        @discardableResult
        mutating func edit(_ messageId: String, body: ChatMessageBody, at editedAt: UInt64) -> Bool {
            update(messageId) {
                $0.body = body
                $0.editedAt = editedAt
            }
        }

        @discardableResult
        mutating func setStatus(_ messageId: String, _ status: ChatMessageStatus) -> Bool {
            update(messageId) { $0.status = status }
        }

        @discardableResult
        mutating func remove(_ messageId: String) -> Bool {
            guard let index = index(of: messageId) else { return false }
            messages.remove(at: index)
            return true
        }

        /// Places or removes one reaction. `byUser` marks the reaction of the user of the host.
        @discardableResult
        mutating func react(_ messageId: String, emoji: String, placed: Bool, byUser: Bool) -> Bool {
            update(messageId) { message in
                var summaries = message.reactions
                let index = summaries.firstIndex { $0.emoji == emoji }

                if placed {
                    if let index {
                        guard !(byUser && summaries[index].reactedByUser) else { return }
                        summaries[index].count += 1
                        summaries[index].reactedByUser = summaries[index].reactedByUser || byUser
                    } else {
                        summaries.append(ChatReactionSummary(emoji: emoji, count: 1, reactedByUser: byUser))
                    }
                } else if let index {
                    guard !byUser || summaries[index].reactedByUser else { return }
                    summaries[index].count -= 1
                    summaries[index].reactedByUser = summaries[index].reactedByUser && !byUser
                    if summaries[index].count < 1 {
                        summaries.remove(at: index)
                    }
                }

                message.reactions = summaries
            }
        }
    }

    private extension MockChatConversation {
        func index(of messageId: String) -> Int? {
            messages.firstIndex { $0.messageId == messageId }
        }

        mutating func update(_ messageId: String, _ change: (inout ChatMessage) -> Void) -> Bool {
            guard let index = index(of: messageId) else { return false }
            change(&messages[index])
            return true
        }
    }
#endif
