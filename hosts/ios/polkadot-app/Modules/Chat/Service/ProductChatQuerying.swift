import Foundation
import TrUAPIHost

/// The two queries that a product-source room asks of its product.
///
/// Each call throws ``ChatQueryError`` when the product refuses, fails or does not answer in time.
protocol ProductChatQuerying: Sendable {
    /// Asks the product for one page of messages.
    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem

    /// Sends one action of the user to the product.
    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem
}
