import Foundation
import os
import Testing
import UIKit
@testable import polkadot_app

/// The alerts of a ringing product room outside its room screen (07-LATER §11.2, scenario S73).
@MainActor
struct ProductRoomCallAlertsTests {
    private let room = Chat.Id.chatExtension("game.dot", roomId: "lobby")
    private let store = ChatRoomCallStore()
    private let surface = RecordingCallSurface()
    private let notifications = RecordingCallNotifications()

    @Test func anotherScreenInFrontShowsTheInAppBanner() async {
        let alerts = makeAlerts()
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)

        await alerts.handle(Chat.RoomCallChange(chatId: room, call: store.call(for: room)))

        #expect(surface.banners == [.init(title: "Lobby", subtitle: String(localized: .ChatCall.ringingTitle))])
        #expect(notifications.notified.isEmpty)
    }

    @Test func aTapOnTheBannerOpensTheRoom() async {
        let alerts = makeAlerts()
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: store.call(for: room)))

        surface.tapBanner()

        #expect(surface.opened == [room])
        #expect(surface.banners.isEmpty)
    }

    @Test func theScreenOfTheRoomInFrontShowsNoBanner() async {
        surface.roomInFront = room
        let alerts = makeAlerts()
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)

        await alerts.handle(Chat.RoomCallChange(chatId: room, call: store.call(for: room)))

        #expect(surface.banners.isEmpty)
        #expect(notifications.notified.isEmpty)
    }

    @Test func inTheBackgroundARingingRoomGetsALocalNotification() async {
        surface.isInBackground = true
        let alerts = makeAlerts()
        let call = Chat.RoomCall.ringing(callId: "c1", video: true, caller: nil)

        await alerts.handle(Chat.RoomCallChange(chatId: room, call: call))
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: call))

        let expected = ProductRoomCallNotification(
            identifier: ProductRoomCallAlerts.notificationIdentifier(room),
            title: "Lobby",
            body: String(localized: .ChatCall.ringingTitleVideo),
            deeplink: "polkadot://game.dot/-/chat/lobby"
        )
        #expect(notifications.notified == [expected])
        #expect(surface.banners.isEmpty)
    }

    @Test func theEndOfTheRingingRemovesTheAlerts() async {
        surface.isInBackground = true
        let alerts = makeAlerts()
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: .ringing(callId: "c1", video: false, caller: nil)))

        await alerts.handle(Chat.RoomCallChange(chatId: room, call: nil))

        #expect(notifications.removed == [ProductRoomCallAlerts.notificationIdentifier(room)])
        #expect(surface.hidden.contains(room))
    }

    @Test func aRoomThatRingsWhenTheAppGoesToTheBackgroundGetsANotification() async {
        let alerts = makeAlerts()
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: store.call(for: room)))

        surface.isInBackground = true
        await alerts.notifyRingingRooms()
        await alerts.notifyRingingRooms()

        #expect(notifications.notified.map(\.identifier) == [ProductRoomCallAlerts.notificationIdentifier(room)])
        #expect(surface.banners.isEmpty)
    }

    /// PARITY row 40: the app comes back to the foreground while a room rings. The local notification goes away and
    /// the in-app banner shows.
    @Test func theReturnToTheForegroundReplacesTheNotificationWithTheBanner() async {
        surface.isInBackground = true
        let alerts = makeAlerts()
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: store.call(for: room)))

        await alerts.returnToForeground()

        #expect(notifications.removed == [ProductRoomCallAlerts.notificationIdentifier(room)])
        #expect(surface.banners == [.init(title: "Lobby", subtitle: String(localized: .ChatCall.ringingTitle))])
    }

    /// PARITY row 40: the foreground notification of the app starts the same change. A room whose screen is in front
    /// loses its notification and shows no banner.
    @Test func theForegroundNotificationShowsTheBannerOfARoomThatIsNotInFront() async {
        surface.isInBackground = true
        let center = NotificationCenter()
        let alerts = makeAlerts(notificationCenter: center)
        let other = Chat.Id.chatExtension("game.dot", roomId: "other")
        await alerts.handle(Chat.RoomCallChange(chatId: room, call: .ringing(callId: "c1", video: false, caller: nil)))
        await alerts.handle(Chat.RoomCallChange(chatId: other, call: .ringing(callId: "c2", video: false, caller: nil)))
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        store.set(.ringing(callId: "c2", video: false, caller: nil), for: other)
        surface.roomInFront = other

        center.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        for _ in 0 ..< 400 where notifications.removed.count < 2 || surface.banners.isEmpty {
            try? await Task.sleep(for: .milliseconds(5))
        }

        #expect(Set(notifications.removed) == Set([room, other].map(ProductRoomCallAlerts.notificationIdentifier)))
        #expect(surface.banners == [.init(title: "Lobby", subtitle: String(localized: .ChatCall.ringingTitle))])
    }

    @Test func theRoomLinkEscapesTheRoomId() {
        #expect(ProductRoomCallAlerts.roomLink(productId: "game.dot", roomId: "a b")
            == "polkadot://game.dot/-/chat/a%20b")
    }

    private func makeAlerts(notificationCenter: NotificationCenter = NotificationCenter()) -> ProductRoomCallAlerts {
        let alerts = ProductRoomCallAlerts(
            store: store,
            notifications: notifications,
            roomName: { _ in "Lobby" },
            logger: StubLogger()
        )
        alerts.start(surface: surface, notificationCenter: notificationCenter)
        return alerts
    }
}

@MainActor
final class RecordingCallSurface: ProductRoomCallSurface {
    struct Banner: Equatable {
        let title: String
        let subtitle: String
    }

    var isInBackground = false
    var roomInFront: Chat.Id?
    private(set) var banners: [Banner] = []
    private(set) var hidden: [Chat.Id] = []
    private(set) var opened: [Chat.Id] = []
    private var onOpen: (() -> Void)?

    func showsRoom(_ chatId: Chat.Id) -> Bool {
        roomInFront == chatId
    }

    func showBanner(title: String, subtitle: String, for _: Chat.Id, onOpen: @escaping () -> Void) {
        banners.append(Banner(title: title, subtitle: subtitle))
        self.onOpen = onOpen
    }

    func hideBanner(for chatId: Chat.Id) {
        hidden.append(chatId)
        banners = []
    }

    func openRoom(_ chatId: Chat.Id) {
        opened.append(chatId)
    }

    func tapBanner() {
        onOpen?()
    }
}

final class RecordingCallNotifications: ProductRoomCallNotifying, @unchecked Sendable {
    private let records = OSAllocatedUnfairLock(
        initialState: (notified: [ProductRoomCallNotification](), removed: [String]())
    )

    var notified: [ProductRoomCallNotification] { records.withLock { $0.notified } }
    var removed: [String] { records.withLock { $0.removed } }

    func notify(_ notification: ProductRoomCallNotification) async {
        records.withLock { $0.notified.append(notification) }
    }

    func remove(identifier: String) {
        records.withLock { $0.removed.append(identifier) }
    }
}
