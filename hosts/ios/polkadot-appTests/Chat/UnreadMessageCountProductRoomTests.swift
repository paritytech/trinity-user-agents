import Foundation
import Testing
@testable import polkadot_app

/// The app badge counts the `UnreadCount` badges that products set for their rooms.
struct UnreadMessageCountProductRoomTests {
    private let fixture = ProductRoomFixture()

    @Test func theBadgeOfAProductRoomCounts() async throws {
        try await createRoom("dm-1", badge: .unreadCount(3))
        try await createRoom("dm-2", badge: .unreadCount(2))

        #expect(try await service().totalUnreadBadgeMessageCount() == 5)
    }

    @Test func aRoomWhoseHostCountsKeepsItsRowsAndARoomWithItsOwnBadgeDoesNot() async throws {
        try await createRoom("home", badge: .hostDefault)
        try await createRoom("own", badge: .unreadCount(1))
        _ = try await fixture.surface.sendMessage(.text("welcome"), roomId: "home")
        _ = try await fixture.surface.sendMessage(.text("hello"), roomId: "own")

        #expect(try await service().totalUnreadBadgeMessageCount() == 2)
    }

    @Test func theStreamFollowsTheBadgeOfAProductRoom() async throws {
        try await createRoom("dm-1", badge: .unreadCount(1))
        let counts = service().totalUnreadBadgeMessageCountStream()
        var iterator = counts.makeAsyncIterator()

        #expect(try await iterator.next() == 1)

        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: "dm-1", preview: preview(.unreadCount(4)))
        #expect(try await iterator.next() == 4)

        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: "dm-1", preview: preview(.unreadCount(0)))
        #expect(try await iterator.next() == 0)
    }

    @Test func theTotalSkipsRowsOfRoomsWithTheirOwnBadge() {
        let rows = [
            UnreadBadgeRow(messageId: "1", groupingId: "a", chatIdentifier: "person"),
            UnreadBadgeRow(messageId: "2", groupingId: "a", chatIdentifier: "person"),
            UnreadBadgeRow(messageId: "3", groupingId: "c", chatIdentifier: "own")
        ]
        let rooms = [
            UnreadBadgeRoom(chatIdentifier: "own", setsOwnBadge: true, productCount: 7),
            UnreadBadgeRoom(chatIdentifier: "image", setsOwnBadge: true, productCount: nil)
        ]

        #expect(UnreadMessageCountService.badgeTotal(rows: rows, rooms: rooms) == 8)
    }
}

private extension UnreadMessageCountProductRoomTests {
    func service() -> UnreadMessageCountService {
        UnreadMessageCountService(databaseService: fixture.storageFacade.databaseService)
    }

    func preview(_ badge: Chat.RoomBadge) -> Chat.RoomPreview {
        Chat.RoomPreview(timestamp: nil, badge: badge, lastMessage: nil, unseenReaction: false)
    }

    func createRoom(_ roomId: String, badge: Chat.RoomBadge) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: roomId, preview: preview(badge))
    }
}
