import Foundation
import os
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The P10 entries of the header of a product room: the search action (P10.5) and the mute and archive entries of
/// the menu (P10.10).
@MainActor
struct ChatProductRoomHeaderAdditionsTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()
    private let userStates = RecordingUserStates()

    @Test func aHostStoreRoomOffersSearch() throws {
        _ = try makePresenter(policy: Data.policy(.hostDefault))

        #expect(view.searchItems.count == 1)
        #expect(view.searchItems.last??.primaryAction?.title == String(localized: .ChatExtension.roomSearch))
    }

    /// A product-source room offers search only when it declares `search`, and the action follows the declaration.
    @Test func aProductSourceRoomOffersSearchWhenItDeclaresIt() throws {
        let presenter = try makePresenter(policy: Data.policy(Data.config(productSource: true)))
        var declared = Data.policy(Data.config(productSource: true))
        declared.features = Chat.RoomFeatures(
            preimageAttachments: false, search: true, calls: nil, readAloud: false
        )

        presenter.update(policy: declared, messages: [])

        #expect(view.searchItems.map { $0 != nil } == [false, true])
    }

    @Test func aSearchResultOfAHostStoreRoomScrollsTheRoom() throws {
        let presenter = try makePresenter(policy: Data.policy(.hostDefault))

        presenter.showSearchResult("message-1")

        #expect(view.scrolledTo == ["message-1"])
    }

    @Test func theMenuNamesTheActionThatChangesTheState() throws {
        let presenter = try makePresenter(policy: Data.policy(.hostDefault))
        #expect(titles(presenter) == ["Mute", "Archive"])

        var changed = Data.policy(.hostDefault)
        changed.userState = Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: true)
        presenter.update(policy: changed, messages: [])

        #expect(titles(presenter) == ["Unmute", "Unarchive"])
    }

    @Test func theMenuEntriesChangeTheStateOfTheRoom() async throws {
        let presenter = try makePresenter(policy: Data.policy(.hostDefault))
        var muted = Data.policy(.hostDefault)
        muted.userState = Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: false)
        presenter.update(policy: muted, messages: [])

        for action in presenter.headerMenuActions() {
            if case let .custom(custom) = action { custom.handler() }
        }
        try await userStates.waitForChanges(2)

        #expect(Set(userStates.changes) == [.unmute, .archive(true)])
    }

    @Test func withoutAStateServiceTheMenuHasNoEntries() throws {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            userStates: nil,
            logger: StubLogger()
        ))

        #expect(presenter.headerMenuActions().isEmpty)
    }
}

private extension ChatProductRoomHeaderAdditionsTests {
    func makePresenter(policy: ChatRoomPolicy) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            userStates: userStates,
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.update(policy: policy, messages: [])
        return presenter
    }

    func titles(_ presenter: ChatProductRoomPresenter) -> [String] {
        presenter.headerMenuActions().compactMap { action in
            if case let .custom(custom) = action { custom.titleProvider() } else { nil }
        }
    }
}

/// Records the changes of the user state that the room asks for.
private final class RecordingUserStates: ChatRoomUserStateServicing, @unchecked Sendable {
    private let recorded = OSAllocatedUnfairLock<[ChatRoomUserStateChange]>(initialState: [])

    var changes: [ChatRoomUserStateChange] {
        recorded.withLock { $0 }
    }

    func state(of _: Chat.Id) async throws -> Chat.RoomUserState {
        .default
    }

    func mute(_: Chat.Id, for duration: Chat.MuteDuration) async throws {
        recorded.withLock { $0.append(.mute(duration)) }
    }

    func unmute(_: Chat.Id) async throws {
        recorded.withLock { $0.append(.unmute) }
    }

    func setArchived(_ archived: Bool, for _: Chat.Id) async throws {
        recorded.withLock { $0.append(.archive(archived)) }
    }

    func waitForChanges(_ count: Int) async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while changes.count < count, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
