import Foundation
import UIKitExt
@testable import polkadot_app

/// Accepts every call and does nothing, for a bot that needs no worker.
final class IdleChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    func onUserMessage(text _: String, roomId _: String?) async throws {}

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}
