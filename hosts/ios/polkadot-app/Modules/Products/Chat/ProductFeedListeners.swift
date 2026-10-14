import Foundation
import os
import TrUAPIHost

/// Takes the feed updates that a product pushes for a room whose messages the product keeps
/// (`product_source`), while a room screen shows the room. The host stores none of these updates
/// (02-DESIGN §5.3 step 3, §8.4).
protocol ProductFeedListening: AnyObject, Sendable {
    func apply(roomId: String, update: ChatFeedUpdate) async
}

/// The room screens that show a product-source room now, by chat.
///
/// A screen registers while it shows its room and keeps the registration. The registry holds each listener
/// weakly, and a registration that goes away removes its listener. An update for a room that no screen shows
/// goes nowhere: the product keeps the messages, and a screen asks for them again when it opens.
final class ProductFeedListeners: @unchecked Sendable {
    static let shared = ProductFeedListeners()

    /// Keeps a listener in the registry. Cancel it, or let it go, to remove the listener.
    final class Registration: @unchecked Sendable {
        private let remove = OSAllocatedUnfairLock<(@Sendable () -> Void)?>(initialState: nil)

        fileprivate init(remove: @escaping @Sendable () -> Void) {
            self.remove.withLock { $0 = remove }
        }

        deinit {
            cancel()
        }

        func cancel() {
            let remove = remove.withLock { remove in
                defer { remove = nil }
                return remove
            }
            remove?()
        }
    }

    private struct Entry {
        weak var listener: (any ProductFeedListening)?
    }

    private let entries = OSAllocatedUnfairLock(uncheckedState: [String: [UUID: Entry]]())

    func register(_ listener: any ProductFeedListening, for chatId: Chat.Id) -> Registration {
        let key = chatId.rawRepresentation
        let token = UUID()
        entries.withLockUnchecked { $0[key, default: [:]][token] = Entry(listener: listener) }

        return Registration { [weak self] in
            self?.entries.withLockUnchecked { entries in
                entries[key]?[token] = nil
                if entries[key]?.isEmpty == true {
                    entries[key] = nil
                }
            }
        }
    }

    /// Gives the update to each screen of the room, in turn. Gives false when no screen shows the room.
    func deliver(_ update: ChatFeedUpdate, roomId: String, to chatId: Chat.Id) async -> Bool {
        let listeners = entries.withLockUnchecked { entries in
            (entries[chatId.rawRepresentation] ?? [:]).values.compactMap(\.listener)
        }

        for listener in listeners {
            await listener.apply(roomId: roomId, update: update)
        }

        return !listeners.isEmpty
    }
}
