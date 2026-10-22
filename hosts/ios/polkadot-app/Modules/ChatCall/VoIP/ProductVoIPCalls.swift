import CallKit
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
    /// or the push is no call offer. Throws ``ProductVoIPCallError/blockedContact`` for an offer of a blocked contact,
    /// which the native chat does not ring.
    func input(pushId: String, message: String) async throws -> VoIPCallKitInput?
}

enum ProductVoIPCallError: Error {
    case blockedContact
}

/// Rings the chat product's call offers on the CallKit screen (HI6): a VoIP push opens with the contact's route,
/// and the answer the user gives there reaches the product as the room event `CallAnswered`, which the product keeps
/// until it reads the offer. The product then starts the call engine through `callMedia`.
///
/// The push wakes the worker of the product, so the product reads the offer, as the native chat reads it when the
/// push wakes the app. When the caller stops before the user answered, the product ends the call of the room, and the
/// CallKit screen ends as the native chat ends it.
final class ProductVoIPCalls: ProductVoIPCallResolving, @unchecked Sendable {
    static let shared = ProductVoIPCalls()

    private let routes: ProductPushRoutes
    private let rooms: ProductRoomReading
    private let lock = NSLock()
    private var ringing: ProductVoIPCall?
    /// The product rings the call of ``ringing`` in its room.
    private var productRings = false
    private var wake: (@Sendable (ProductVoIPCall) async -> Void)?
    private var watching: Task<Void, Never>?
    private var watchingRoomCalls: Task<Void, Never>?

    init(
        routes: ProductPushRoutes = ProductPushRoutes(),
        rooms: ProductRoomReading = ProductRoomReader(storageFacade: UserDataStorageFacade.shared)
    ) {
        self.routes = routes
        self.rooms = rooms
    }

    func input(pushId: String, message: String) async throws -> VoIPCallKitInput? {
        guard let route = routes.route(pushId: pushId.lowercased()),
              let title = route.chatTitle,
              let sealed = try? Data(hexString: message),
              case let .call(messageId, video) = decodeChatPush(key: route.key, message: sealed)
        else { return nil }

        let chatId = Chat.Id.chatExtension(route.productId, roomId: route.roomId)
        guard await rooms.declarations(of: chatId)?.contact?.state != .blocked else {
            throw ProductVoIPCallError.blockedContact
        }

        let call = ProductVoIPCall(productId: route.productId, roomId: route.roomId, callId: messageId)
        let wake = lock.withLock {
            ringing = call
            productRings = false
            return self.wake
        }
        if let wake {
            Task { await wake(call) }
        }
        return VoIPCallKitInput(name: title, callType: video ? .video : .audio)
    }

    /// The offer that rings now, if the chat product's.
    var ringingCall: ProductVoIPCall? {
        lock.withLock { ringing }
    }

    /// Wakes the worker of the product of each offer that rings.
    func wakeWorker(with wake: @escaping @Sendable (ProductVoIPCall) async -> Void) {
        lock.withLock { self.wake = wake }
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

    /// Ends the CallKit screen of the ringing offer when the product ends its call before the user answered: the
    /// caller stopped, as the native chat ends the CallKit screen when the caller closes the call.
    func watchRoomCalls(of store: ChatRoomCallStoring, callKit: VoIPCallKitManaging) {
        watchingRoomCalls?.cancel()
        watchingRoomCalls = Task { [weak self] in
            for await change in store.changes() where self?.callerStopped(change) == true {
                callKit.markCallDisconnected(with: .remoteEnded)
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

    /// True once when the product ends the ringing call of the room before the user answered it.
    func callerStopped(_ change: Chat.RoomCallChange) -> Bool {
        guard case let .chatExtension(productId, roomId?) = change.chatId else { return false }

        return lock.withLock {
            guard let ringing, ringing.productId == productId, ringing.roomId == roomId else { return false }

            switch change.call {
            case let .ringing(callId, _, _) where callId == ringing.callId:
                productRings = true
                return false
            case nil where productRings:
                self.ringing = nil
                productRings = false
                return true
            default:
                return false
            }
        }
    }
}
