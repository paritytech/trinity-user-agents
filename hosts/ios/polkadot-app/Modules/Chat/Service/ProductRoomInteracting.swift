import Foundation
import TrUAPIHost

/// The questions that a room screen asks the product of the room (02-DESIGN §3.3.7 to §3.3.9, P6).
///
/// Each question throws ``ChatQueryError`` when the product refuses it, fails or does not answer in the lookup time
/// (`ChatQueryTimeout.lookup`). Then the room screen uses the host default for the item.
protocol ProductRoomInteracting: Sendable {
    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem

    /// Every text that a message had, oldest first, for the edit history of the message.
    func editHistory(_ request: ProductChatEditHistoryQueryRequest) async throws -> ProductChatEditHistoryQueryItem
}

/// The product of the room serves none of the questions, for example a product on the native runtime.
struct UnservedProductRoom: ProductRoomInteracting {
    struct NotServed: Error {}

    func reactions(_: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        throw NotServed()
    }

    func editHistory(_: ProductChatEditHistoryQueryRequest) async throws -> ProductChatEditHistoryQueryItem {
        throw NotServed()
    }
}
