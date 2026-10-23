import Foundation
import os
import Products
import UIKitExt
@testable import polkadot_app

final class MockChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    private struct State {
        var startCount = 0
        var attachCount = 0
        var disposeCount = 0
        var userMessages: [String] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var startCount: Int { state.withLock { $0.startCount } }
    var attachCount: Int { state.withLock { $0.attachCount } }
    var disposeCount: Int { state.withLock { $0.disposeCount } }
    var userMessages: [String] { state.withLock { $0.userMessages } }

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {
        state.withLock { $0.startCount += 1 }
    }

    func onUserMessage(text: String, roomId _: String?) async throws {
        state.withLock { $0.userMessages.append(text) }
    }

    func renderMessage(
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<String, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(roomId _: String?, messageId _: String, actionId _: String, payload _: String?) async {}

    @MainActor
    func attach(presentationView _: ControllerBackedProtocol) {
        state.withLock { $0.attachCount += 1 }
    }

    func dispose() async {
        state.withLock { $0.disposeCount += 1 }
    }
}
