import Foundation
import Products
import TrUAPIHost
import UIKitExt

/// A product runtime: boots a JS environment for one product and owns its
/// lifecycle. Implementations are one-per-runtime-mode (native / rust).
///
/// Ownership contract: the reference holder MUST call `dispose()` before
/// releasing a started runtime — there is no deinit safety net. Holders
/// guarantee it from their own deinit (`Task { await runtime.dispose() }`),
/// which the `Sendable` bound makes legal.
protocol ProductRuntimeProtocol: AnyObject, Sendable {
    func dispose() async
}

/// One rendered widget update. The native runtime's JS renderer emits the
/// SCALE-encoded tree as hex; the rust runtime receives typed nodes from the
/// core, which have no Swift SCALE encoder — so the stream carries whichever
/// form the runtime produced and the consumer resolves both to a widget node.
enum ChatRendererOutput: Sendable {
    case scaleEncoded(String)
    case native(RendererNode)
}

/// Chat-environment runtime surface consumed by ProductBot.
protocol ChatRuntimeProtocol: ProductRuntimeProtocol {
    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws
    func onUserMessage(text: String, roomId: String?) async throws
    func renderMessage(roomId: String?, messageId: String, messageType: String, messageData: Data) async
        -> AsyncThrowingStream<ChatRendererOutput, Error>
    func dispatchEvent(
        roomId: String?,
        messageId: String,
        messageType: String?,
        actionId: String,
        payload: String?
    ) async
    /// Reports a press on a button that the host drew for a product `Actions` message.
    func dispatchActionTrigger(roomId: String?, messageId: String, actionId: String) async
    /// Reports what the user did in a room that the host stores. A runtime sends the events that its
    /// protocol version carries.
    func publishUserEvent(_ event: ProductChatUserEvent, roomId: String?) async throws
    @MainActor func attach(presentationView: ControllerBackedProtocol)
}

extension ChatRuntimeProtocol {
    /// A runtime that has no user events drops them.
    func publishUserEvent(_: ProductChatUserEvent, roomId _: String?) async throws {}
}

protocol SPARuntimeProtocol: ProductRuntimeProtocol {
    func start(with engine: JSEngineProtocol) async throws -> URL
}

/// One scripts protocol per surface (see `ChatScriptsMaking` in the Products
/// package); implemented by the native and rust SPA scripts factories.
protocol SPAScriptsMaking {
    func makeScripts() throws -> [JSEngineScript]
}

/// Creation also anchors the context's routers to the presentation view the
/// factory received, so a created runtime can prompt immediately.
protocol SPARuntimeFactoryProtocol: AnyObject {
    @MainActor func createRuntime(for productId: ProductId) throws -> SPARuntimeProtocol
}
