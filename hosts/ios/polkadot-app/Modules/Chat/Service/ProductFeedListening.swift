import Foundation
import os
import TrUAPIHost

/// Receives the feed updates of a product-source room while its room screen is open.
///
/// The bridge does not keep these updates (`persist: false`). It gives each one to the listener of its room.
protocol ProductFeedListening: AnyObject, Sendable {
    func apply(roomId: String, update: ChatFeedUpdate)
}

/// Keeps the listeners of the open product-source rooms, by product and room.
///
/// The registry keeps weak references, so a closed room screen goes away without a call.
final class ProductFeedListenerRegistry: Sendable {
    static let shared = ProductFeedListenerRegistry()

    private struct Key: Hashable, Sendable {
        let productId: String
        let roomId: String
    }

    private struct WeakListener: Sendable {
        weak var listener: ProductFeedListening?
    }

    private let listeners = OSAllocatedUnfairLock<[Key: WeakListener]>(initialState: [:])

    /// The registry gives the updates of the room to the listener until the token goes away.
    func register(
        _ listener: ProductFeedListening,
        productId: String,
        roomId: String
    ) -> ProductFeedRegistration {
        let key = Key(productId: productId, roomId: roomId)
        let identifier = ObjectIdentifier(listener)
        listeners.withLock { $0[key] = WeakListener(listener: listener) }

        return ProductFeedRegistration { [listeners] in
            listeners.withLock { state in
                let current = state[key]?.listener
                if current == nil || current.map(ObjectIdentifier.init) == identifier {
                    state[key] = nil
                }
            }
        }
    }

    /// Gives the update to the listener of the room. Gives false when no screen shows the room.
    @discardableResult
    func apply(productId: String, roomId: String, update: ChatFeedUpdate) -> Bool {
        let listener = listeners.withLock { $0[Key(productId: productId, roomId: roomId)]?.listener }
        listener?.apply(roomId: roomId, update: update)
        return listener != nil
    }
}

/// Removes a listener from the registry when the last reference to it goes away.
final class ProductFeedRegistration: Sendable {
    private let cancel: @Sendable () -> Void

    init(cancel: @escaping @Sendable () -> Void) {
        self.cancel = cancel
    }

    deinit {
        cancel()
    }
}
