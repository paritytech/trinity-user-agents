import Foundation
import os
import TrUAPIHost
@testable import polkadot_app

/// Answers the queries of a product-source room with the closures of a test, and records each request.
final class ScriptedProductChatQuerying: ProductChatQuerying {
    typealias PageAnswer = @Sendable (ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryResponse
    typealias ActionAnswer = @Sendable (ProductChatUserActionRequest) async throws -> ProductChatUserActionResponse

    private let page: PageAnswer
    private let action: ActionAnswer
    private let pages = OSAllocatedUnfairLock<[ProductChatMessagesQueryRequest]>(initialState: [])
    private let actions = OSAllocatedUnfairLock<[ProductChatUserActionRequest]>(initialState: [])

    init(
        page: @escaping PageAnswer = { _ in ProductChatMessagesQueryResponse(messages: [], next: nil) },
        action: @escaping ActionAnswer = { _ in .acknowledged }
    ) {
        self.page = page
        self.action = action
    }

    var pageRequests: [ProductChatMessagesQueryRequest] {
        pages.withLock { $0 }
    }

    var actionRequests: [ProductChatUserActionRequest] {
        actions.withLock { $0 }
    }

    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryResponse {
        pages.withLock { $0.append(request) }
        return try await page(request)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionResponse {
        actions.withLock { $0.append(request) }
        return try await action(request)
    }
}
