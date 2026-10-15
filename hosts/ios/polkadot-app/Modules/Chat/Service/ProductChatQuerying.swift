import Foundation
import TrUAPIHost

/// The two queries that a product-source room asks of its product (02-DESIGN §3.3.5, §3.3.6).
///
/// Each call throws ``ChatQueryError`` when the product refuses, fails or does not answer in time.
protocol ProductChatQuerying: Sendable {
    /// Asks the product for one page of messages.
    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem

    /// Sends one action of the user to the product.
    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem
}

/// Asks the queries through the execution of the worker of the product, with the timeouts of 02-DESIGN §3.6.
struct ProductExecutionChatQuerying: ProductChatQuerying {
    /// Gives the execution of the worker. It throws when the worker does not run.
    let execution: @Sendable () async throws -> any TrUAPIProductExecutionProtocol

    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        try await execution().chatMessagesQuery(request, timeout: ChatQueryTimeout.messages)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        try await execution().chatUserAction(request, timeout: ChatQueryTimeout.timeout(for: request.action))
    }
}

/// The search of a product-source room that declares `search` (07-LATER §8.2). The host asks the product, because
/// the product keeps the messages of the room.
///
/// Each call throws ``ChatQueryError`` when the product refuses, fails or does not answer in 10 s.
protocol ProductChatSearching: Sendable {
    func searchPage(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem
}
