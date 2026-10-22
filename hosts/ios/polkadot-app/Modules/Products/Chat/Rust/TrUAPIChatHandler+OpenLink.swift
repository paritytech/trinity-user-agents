import Foundation
import TrUAPIHost

/// Something that the user opened from an entry point of the host, such as a contact, goes to the worker of the
/// product, which starts first when it does not run. The handler holds the worker until the product answers.
extension TrUAPIChatHandler: ProductChatLinkTaking {
    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem {
        try await withWorker { try await $0.chatOpenLink(request, timeout: ChatQueryTimeout.openLink) }
    }
}
