import Foundation
import TrUAPIHost

/// Something that the user opened from an entry point of the host, such as a contact, goes to the worker of the
/// product, which starts first when it does not run. The handler holds the worker until the product answers.
///
/// A link that opens the app, or brings it back from the background, reaches a worker that is still starting: its
/// client connects to the core only after the worker runs (`NotConnected`), and the chat product answers
/// "chat is starting" until its surface runs. Both are waited out for ``messageStartupWindow``, as a user message
/// waits for its worker; any other answer is the product's own and is passed on at once.
extension TrUAPIChatHandler: ProductChatLinkTaking {
    /// How often a link asks again while the worker of the product starts.
    static let linkRetryInterval = Duration.milliseconds(250)

    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem {
        let deadline = ContinuousClock.now + messageStartupWindow
        return try await withWorker { execution in
            while true {
                do {
                    return try await execution.chatOpenLink(request, timeout: ChatQueryTimeout.openLink)
                } catch let error where Self.isStarting(error) && ContinuousClock.now < deadline {
                    try await Task.sleep(for: Self.linkRetryInterval)
                }
            }
        }
    }

    /// The answer of a worker whose client or chat surface does not run yet.
    static func isStarting(_ error: Error) -> Bool {
        if (error as? ProductRuntimeError) == .NotConnected {
            return true
        }
        return (error as? ChatQueryError) == .refused(.unknown(reason: chatSurfaceStarting))
    }

    /// The reason of the chat product's refusal while its surface does not run (`registerChatHandlers`).
    static let chatSurfaceStarting = "chat is starting"
}
