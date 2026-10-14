import Foundation
import Products
import UIKitExt

/// The chat-driving surface of a running worker. Chat is the only consumer that
/// talks to the worker's JS; SPA and operations only keep it alive through a
/// `ProductWorkerToken`, so they need nothing beyond `ProductWorkerRunning`.
protocol ProductChatWorking: ProductWorkerRunning {
    func bindMessaging(_ support: ProductsNativeApi.MessagingSupport)
    func unbindMessaging()
    func onBotStarted() async throws
    func onUserMessage(text: String, roomId: String?) async throws
    func renderMessage(
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<String, Error>
    func dispatchEvent(roomId: String?, messageId: String, actionId: String, payload: String?) async
    @MainActor func attach(presentationView: ControllerBackedProtocol)
}

/// One product's headless worker: a booted ``ProductsScriptExecutor`` plus the
/// ``ProductsNativeApi`` behind it. The manager owns exactly one per product and
/// disposes it when the last lock is released.
///
/// `@unchecked Sendable`: only immutable wiring is stored here; the engine state
/// lives inside the executor actor and the chat binding inside the native API's
/// own lock.
final class ProductScriptWorker: ProductChatWorking, @unchecked Sendable {
    private let scriptExecutor: ProductsScriptExecutorProtocol
    private let nativeApi: ProductsNativeApi
    private let routers: ProductRoutersFacadeProtocol

    init(
        scriptExecutor: ProductsScriptExecutorProtocol,
        nativeApi: ProductsNativeApi,
        routers: ProductRoutersFacadeProtocol
    ) {
        self.scriptExecutor = scriptExecutor
        self.nativeApi = nativeApi
        self.routers = routers
    }

    func dispose() async {
        await scriptExecutor.dispose()
    }

    func bindMessaging(_ support: ProductsNativeApi.MessagingSupport) {
        nativeApi.bindMessaging(support)
    }

    func unbindMessaging() {
        nativeApi.unbindMessaging()
    }

    func onBotStarted() async throws {
        try await scriptExecutor.onBotStarted()
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        try await scriptExecutor.onUserMessage(text: text, roomId: roomId)
    }

    func renderMessage(
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<String, Error> {
        await scriptExecutor.renderMessage(
            messageId: messageId,
            messageType: messageType,
            messageData: messageData
        )
    }

    func dispatchEvent(roomId: String?, messageId: String, actionId: String, payload: String?) async {
        await scriptExecutor.dispatchEvent(
            roomId: roomId,
            messageId: messageId,
            actionId: actionId,
            payload: payload
        )
    }

    @MainActor
    func attach(presentationView view: ControllerBackedProtocol) {
        routers.setPresentationView(view)
    }
}
