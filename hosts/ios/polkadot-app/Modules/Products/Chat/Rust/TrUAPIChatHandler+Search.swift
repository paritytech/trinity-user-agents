import Foundation
import TrUAPIHost

/// The search of a product-source room asks the worker of the product, and holds the worker while it runs.
extension TrUAPIChatHandler: ProductChatSearching {
    func searchPage(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem {
        try await withWorker { try await $0.chatSearchQuery(request, timeout: ChatQueryTimeout.search) }
    }
}

/// The search of the chat list asks the worker only while it runs.
extension TrUAPIChatHandler: ProductChatListSearching {
    func searchPageIfRunning(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem? {
        guard workers.currentExecution(of: productId) != nil else {
            return nil
        }
        return try await searchPage(request)
    }
}
