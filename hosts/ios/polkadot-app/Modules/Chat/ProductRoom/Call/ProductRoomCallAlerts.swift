import Foundation
import Operation_iOS
import UIKit

/// What the alerts of a ringing room read of the app and show in it.
@MainActor
protocol ProductRoomCallSurface: AnyObject {
    var isInBackground: Bool { get }

    /// True when the screen of the room is in front.
    func showsRoom(_ chatId: Chat.Id) -> Bool

    /// Shows the in-app banner of a ringing room. A tap on the banner calls `onOpen`.
    func showBanner(title: String, subtitle: String, for chatId: Chat.Id, onOpen: @escaping () -> Void)

    /// Removes the in-app banner of the room, when it shows.
    func hideBanner(for chatId: Chat.Id)

    /// Opens the screen of the room.
    func openRoom(_ chatId: Chat.Id)
}

/// Shows and removes the local notification of a ringing room.
protocol ProductRoomCallNotifying: Sendable {
    func notify(_ notification: ProductRoomCallNotification) async

    func remove(identifier: String)
}

/// The local notification of a ringing room (07-LATER §11.2).
struct ProductRoomCallNotification: Equatable {
    let identifier: String
    let title: String
    let body: String
    /// The link of the room, which a tap on the notification opens.
    let deeplink: String
}

/// The alerts of a ringing product room outside its room screen (07-LATER §11.2, scenario S73): an in-app banner when
/// another screen of the app is in front, and a time-sensitive local notification when the app is in the background.
/// A ringing call that is in the store when the app goes to the background also gets the notification. When the app
/// comes back to the foreground, the notification of a room that still rings goes away and the in-app banner shows.
///
/// A product rings only while its worker runs, so a room of a closed app does not ring (no CallKit, D31).
@MainActor
final class ProductRoomCallAlerts {
    static let shared = ProductRoomCallAlerts()

    private let store: ChatRoomCallStoring
    private let notifications: ProductRoomCallNotifying
    private let roomName: @Sendable (Chat.Id) async -> String?
    private let logger: LoggerProtocol
    private weak var surface: ProductRoomCallSurface?
    private var task: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []
    /// The ringing calls that have a local notification, by room.
    private var notified: [Chat.Id: String] = [:]

    nonisolated init(
        store: ChatRoomCallStoring = ChatRoomCallStore.shared,
        notifications: ProductRoomCallNotifying = ProductRoomCallLocalNotifications(),
        roomName: @escaping @Sendable (Chat.Id) async -> String? = ProductRoomCallAlerts.storedRoomName,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.notifications = notifications
        self.roomName = roomName
        self.logger = logger
    }

    /// Follows the calls of the product rooms. The app calls it once at its start.
    func start(
        surface: ProductRoomCallSurface = ProductRoomCallWindowSurface.shared,
        notificationCenter: NotificationCenter = .default
    ) {
        guard task == nil else { return }
        self.surface = surface

        task = Task { [weak self, store] in
            for await change in store.changes() {
                await self?.handle(change)
            }
        }
        observers = [
            notificationCenter.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    Task { await self.notifyRingingRooms() }
                }
            },
            notificationCenter.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    Task { await self.returnToForeground() }
                }
            }
        ]
    }

    func stop() {
        task?.cancel()
        task = nil
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
    }

    /// Shows or removes the alerts of one room after a change of its call.
    func handle(_ change: Chat.RoomCallChange) async {
        guard case let .ringing(callId, video, _) = change.call else {
            removeAlerts(of: change.chatId)
            return
        }
        guard let surface else { return }

        if surface.isInBackground {
            await notify(change.chatId, callId: callId, video: video)
        } else {
            await showInApp(change.chatId, callId: callId, video: video)
        }
    }

    /// Removes the local notification of each ringing room and shows the in-app banner, because the app came back
    /// to the foreground (PARITY row 40). The app is still in the background when it tells this, so the alerts do
    /// not read the state of the app here.
    func returnToForeground() async {
        for change in store.allCalls() {
            guard case let .ringing(callId, video, _) = change.call else {
                continue
            }
            if notified.removeValue(forKey: change.chatId) != nil {
                notifications.remove(identifier: Self.notificationIdentifier(change.chatId))
            }
            await showInApp(change.chatId, callId: callId, video: video)
        }
    }

    /// Gives a local notification to each ringing room that has none, because the app went to the background.
    func notifyRingingRooms() async {
        for change in store.allCalls() {
            guard case let .ringing(callId, video, _) = change.call, notified[change.chatId] != callId else {
                continue
            }
            surface?.hideBanner(for: change.chatId)
            await notify(change.chatId, callId: callId, video: video)
        }
    }
}

private extension ProductRoomCallAlerts {
    /// Shows the in-app banner of a ringing room, or nothing when the screen of the room is in front.
    func showInApp(_ chatId: Chat.Id, callId: String, video: Bool) async {
        guard let surface else { return }

        guard !surface.showsRoom(chatId) else {
            mark(chatId, "room \(callId)")
            return
        }

        let title = await roomName(chatId) ?? chatId.extensionId ?? ""
        guard case .ringing = store.call(for: chatId) else { return }
        surface.showBanner(
            title: title,
            subtitle: ChatRoomCallViewModel.ringingTitle(video: video),
            for: chatId
        ) { [weak surface] in
            surface?.hideBanner(for: chatId)
            surface?.openRoom(chatId)
        }
        mark(chatId, "banner \(callId)")
    }

    func notify(_ chatId: Chat.Id, callId: String, video: Bool) async {
        guard let productId = chatId.extensionId, let roomId = chatId.roomId, notified[chatId] != callId else {
            return
        }

        notified[chatId] = callId
        let title = await roomName(chatId) ?? productId
        await notifications.notify(ProductRoomCallNotification(
            identifier: Self.notificationIdentifier(chatId),
            title: title,
            body: ChatRoomCallViewModel.ringingTitle(video: video),
            deeplink: Self.roomLink(productId: productId, roomId: roomId)
        ))
        mark(chatId, "notification \(callId)")
    }

    func removeAlerts(of chatId: Chat.Id) {
        surface?.hideBanner(for: chatId)
        if notified.removeValue(forKey: chatId) != nil {
            notifications.remove(identifier: Self.notificationIdentifier(chatId))
        }
        mark(chatId, "none")
    }

    func mark(_ chatId: Chat.Id, _ value: String) {
        #if targetEnvironment(simulator)
            guard TrUAPIE2EMarkers.isEnabled, let roomId = chatId.roomId else { return }
            logger.debug("[e2e:ui] call-alert \(roomId) \(value)")
        #endif
    }
}

extension ProductRoomCallAlerts {
    static func notificationIdentifier(_ chatId: Chat.Id) -> String {
        "product-room-call." + chatId.rawRepresentation
    }

    /// The link of a product room (02-DESIGN §13 item 15).
    static func roomLink(productId: String, roomId: String) -> String {
        let room = roomId.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? roomId
        return "polkadot://\(productId)/-/chat/\(room)"
    }

    /// The name of the room in the chat store.
    @Sendable
    nonisolated static func storedRoomName(_ chatId: Chat.Id) async -> String? {
        let repository = UserDataStorageFacade.shared.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        let chat = try? await repository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
        return chat?.roomMetadata?.name
    }
}
