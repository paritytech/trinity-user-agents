import Foundation
import Products

enum ScriptExecutorCall: Equatable {
    case initializeBot
    case onBotStarted
    case onUserMessage
    case dispatchEvent
    case dispose
}

final class MockScriptExecutor: ProductsScriptExecutorProtocol, @unchecked Sendable {
    var calls: [ScriptExecutorCall] = []
    var lastUserMessage: (text: String, roomId: String?)?

    func initializeBot(nativeApi _: any ProductsNativeApiProtocol) async throws {
        calls.append(.initializeBot)
    }

    func onBotStarted() async throws {
        calls.append(.onBotStarted)
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        calls.append(.onUserMessage)
        lastUserMessage = (text, roomId)
    }

    func renderMessage(
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<String, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(roomId _: String?, messageId _: String, actionId _: String, payload _: String?) async {
        calls.append(.dispatchEvent)
    }

    func dispose() async {
        calls.append(.dispose)
    }
}
