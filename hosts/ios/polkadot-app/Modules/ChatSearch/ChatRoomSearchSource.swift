import Foundation
import TrUAPIHost

/// Where the next page of an in-room search starts.
enum ChatRoomSearchCursor: Equatable {
    /// After a row of the store of the host.
    case store(ChatSearchCursor)
    /// The cursor that the product gave.
    case product(String)
}

struct ChatRoomSearchPage: Equatable {
    /// Newest first.
    let hits: [ChatSearchHit]
    /// Nil when no older matches exist.
    let next: ChatRoomSearchCursor?
}

enum ChatRoomSearchError: Error, Equatable {
    /// The product answered `NotAvailable`, failed, or did not answer in time. The screen shows "Search is not
    /// available".
    case unavailable
}

/// The search of one room: the store of the host for a host-store room, the product for a product-source room
/// (07-LATER §8.2).
protocol ChatRoomSearching: Sendable {
    /// How long the field waits after the last key before it asks.
    var typingPause: Duration { get }

    func page(matching text: String, after cursor: ChatRoomSearchCursor?) async throws -> ChatRoomSearchPage
}

/// Searches the messages that the host stores for the room.
struct StoreChatRoomSearch: ChatRoomSearching {
    let chatId: Chat.Id
    let service: ChatSearchServicing

    var typingPause: Duration {
        .milliseconds(150)
    }

    func page(matching text: String, after cursor: ChatRoomSearchCursor?) async throws -> ChatRoomSearchPage {
        let storeCursor: ChatSearchCursor? = if case let .store(cursor) = cursor { cursor } else { nil }
        let page = try await service.searchMessages(matching: text, in: chatId, after: storeCursor)
        return ChatRoomSearchPage(hits: page.hits, next: page.next.map(ChatRoomSearchCursor.store))
    }
}

/// Asks the product of a product-source room with `search_query`, 30 hits a page.
struct ProductChatRoomSearch: ChatRoomSearching {
    static let pageSize: UInt32 = 30

    let chatId: Chat.Id
    let roomId: String
    let searching: ProductChatSearching
    let logger: LoggerProtocol

    var typingPause: Duration {
        .milliseconds(300)
    }

    func page(matching text: String, after cursor: ChatRoomSearchCursor?) async throws -> ChatRoomSearchPage {
        let productCursor: String? = if case let .product(cursor) = cursor { cursor } else { nil }
        let request = ProductChatSearchQueryRequest(
            roomId: roomId,
            text: text,
            cursor: productCursor,
            pageSize: Self.pageSize
        )

        let answer: ProductChatSearchQueryItem
        do {
            answer = try await searching.searchPage(request)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.warning("Product search of room \(roomId) is not available: \(error)")
            throw ChatRoomSearchError.unavailable
        }

        let extensionId = chatId.extensionId ?? ""
        let hits = answer.hits.prefix(Int(Self.pageSize)).map { message in
            let local = message.localMessage(chatId: chatId, extensionId: extensionId, isUnread: false)
            return ChatSearchHit(
                chatId: chatId,
                messageId: local.messageId,
                timestamp: local.timestamp,
                text: local.content.searchSource ?? "",
                isOutgoing: message.isFromUser,
                authorParticipantId: local.authorParticipantId
            )
        }
        return ChatRoomSearchPage(hits: hits, next: answer.next.map(ChatRoomSearchCursor.product))
    }
}
