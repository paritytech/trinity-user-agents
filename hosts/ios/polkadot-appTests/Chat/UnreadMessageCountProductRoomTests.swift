import CoreData
import Foundation
import Testing
@testable import polkadot_app

/// The app badge counts the `UnreadCount` badges that products set for their rooms, and nothing of a muted room
/// (VERBOSITY-WASM-PRODUCT C4).
struct UnreadMessageCountProductRoomTests {
    private static let now = Date(timeIntervalSince1970: 1_800_000_000)
    private let fixture = ProductRoomFixture()

    @Test func theBadgeOfAProductRoomCounts() async throws {
        try await createRoom("dm-1", badge: .unreadCount(3))
        try await createRoom("dm-2", badge: .unreadCount(2))

        #expect(try await service().totalUnreadBadgeMessageCount() == 5)
    }

    @Test func aMutedRoomAddsNothingUntilTheMuteEnds() async throws {
        try await createRoom("dm-1", badge: .unreadCount(3))
        try await createRoom("dm-2", badge: .unreadCount(2))
        try await setMutedUntil(Int64.max, room: "dm-1")

        #expect(try await service().totalUnreadBadgeMessageCount() == 2)

        let oneHourAgo = Int64(Self.now.addingTimeInterval(-3_600).toChatTimestamp())
        try await setMutedUntil(oneHourAgo, room: "dm-1")

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

    @Test func theTotalSkipsMutedChatsAndRowsOfRoomsWithTheirOwnBadge() {
        let rows = [
            UnreadBadgeRow(messageId: "1", groupingId: "a", chatIdentifier: "person"),
            UnreadBadgeRow(messageId: "2", groupingId: "a", chatIdentifier: "person"),
            UnreadBadgeRow(messageId: "3", groupingId: "b", chatIdentifier: "muted"),
            UnreadBadgeRow(messageId: "4", groupingId: "c", chatIdentifier: "own")
        ]
        let rooms = [
            UnreadBadgeRoom(chatIdentifier: "muted", setsOwnBadge: false, productCount: nil, storedMutedUntil: .max),
            UnreadBadgeRoom(chatIdentifier: "own", setsOwnBadge: true, productCount: 7, storedMutedUntil: 0),
            UnreadBadgeRoom(chatIdentifier: "image", setsOwnBadge: true, productCount: nil, storedMutedUntil: 0)
        ]

        #expect(UnreadMessageCountService.badgeTotal(rows: rows, rooms: rooms, at: Self.now) == 8)
    }
}

private extension UnreadMessageCountProductRoomTests {
    func service() -> UnreadMessageCountService {
        UnreadMessageCountService(databaseService: fixture.storageFacade.databaseService, now: { Self.now })
    }

    func preview(_ badge: Chat.RoomBadge) -> Chat.RoomPreview {
        Chat.RoomPreview(order: .byTimestamp, timestamp: nil, badge: badge, text: nil, ongoing: false)
    }

    func createRoom(_ roomId: String, badge: Chat.RoomBadge) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: roomId, preview: preview(badge))
    }

    func setMutedUntil(_ stored: Int64, room roomId: String) async throws {
        let identifier = fixture.chatId(roomId).rawRepresentation
        try await fixture.storageFacade.databaseService.performWrite { context in
            let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
            request.predicate = .chat(for: identifier)
            let metadata = try #require(try context.fetch(request).first?.roomMetadata)
            metadata.mutedUntil = stored
            try context.save()
        }
    }
}
