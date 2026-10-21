import Foundation
import SubstrateSdk
import TrUAPIHost

/// A call offer of the chat product that rang through PushKit: the room it belongs to, and its call id.
struct ProductVoIPCall: Equatable {
    let productId: String
    let roomId: String
    let callId: String
}

protocol ProductVoIPCallResolving: Sendable {
    /// The CallKit input of a VoIP push of the chat product, or nil when no chat route of this device has its push id
    /// or the push is no call offer.
    func input(pushId: String, message: String) -> VoIPCallKitInput?
}

/// Rings the chat product's call offers on the CallKit screen (HI6): a VoIP push opens with the contact's route,
/// and the answer the user gives there reaches the product as the room event `CallAnswered`, which the product keeps
/// until it reads the offer. The product then starts the call engine through `callMedia`.
final class ProductVoIPCalls: ProductVoIPCallResolving, @unchecked Sendable {
    static let shared = ProductVoIPCalls()

    private let routes: ProductPushRoutes
    private let lock = NSLock()
    private var ringing: ProductVoIPCall?
    private var watching: Task<Void, Never>?

    init(routes: ProductPushRoutes = ProductPushRoutes()) {
        self.routes = routes
    }

    func input(pushId: String, message: String) -> VoIPCallKitInput? {
        guard let route = routes.route(pushId: pushId.lowercased()),
              let title = route.chatTitle,
              let sealed = try? Data(hexString: message),
              case let .call(messageId, video) = decodeChatPush(key: route.key, message: sealed)
        else { return nil }
        lock.withLock {
            ringing = ProductVoIPCall(productId: route.productId, roomId: route.roomId, callId: messageId)
        }
        return VoIPCallKitInput(name: title, callType: video ? .video : .audio)
    }

    /// The offer that rings now, if the chat product's.
    var ringingCall: ProductVoIPCall? {
        lock.withLock { ringing }
    }

    /// Gives the product the answer the user gives to its ringing offer on the CallKit screen.
    func watchAnswers(of callKit: VoIPCallKitManaging, publisher: ChatRoomEventPublishing) {
        watching?.cancel()
        watching = Task { [weak self] in
            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    do {
                        for try await pending in callKit.observeHasPendingAnswer() where pending {
                            self?.answer(accept: true, publisher: publisher)
                        }
                    } catch {}
                }
                group.addTask {
                    do {
                        for try await pending in callKit.observeHasPendingEnd() where pending {
                            self?.answer(accept: false, publisher: publisher)
                        }
                    } catch {}
                }
            }
        }
    }

    func answer(accept: Bool, publisher: ChatRoomEventPublishing) {
        let call = lock.withLock {
            defer { ringing = nil }
            return ringing
        }
        guard let call else { return }
        try? publisher.publish(
            .callAnswered(callId: call.callId, accept: accept),
            productId: call.productId,
            roomId: call.roomId
        )
    }
}
