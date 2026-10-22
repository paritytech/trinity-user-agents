import Foundation
import PolkadotUI
import Products
import Testing
@testable import polkadot_app

/// The host defaults of 02-DESIGN §4 for a product room that declares nothing.
struct ChatRoomPolicyTests {
    private let policy = ChatRoomPolicy.defaults

    @Test func theHostStoresTheMessages() {
        #expect(policy.messageStore == .host)
        #expect(policy.hostAppliesUserActions)
    }

    @Test func theListShowsTheHostUnreadCount() {
        #expect(policy.listBadge == .hostUnreadCount)
        #expect(policy.listBadgeCount(hostUnreadCount: 3) == 3)
    }

    @Test func seenMessagesAreNotReported() {
        #expect(!policy.reportsSeen)
    }

    @Test func whoReactedIsNotOffered() {
        #expect(!policy.offersReactionDetails)
    }

    @Test func thereIsNoToolbar() {
        #expect(policy.toolbar.isEmpty)
        #expect(makeBot().peerMetadata.moreActions.isEmpty)
    }

    @Test func theFooterIsTheInput() {
        #expect(policy.footer == .input)
    }

    /// A room without `hop_attachments` cannot take a photo or a video, so the input shows no attachment button.
    @Test func theInputShowsNoAttachmentButtonWithoutHopAttachments() {
        #expect(!policy.sendsMedia)
    }

    @Test func anEmptyFooterHidesTheInput() {
        let room = room(footer: .empty)

        #expect(room.roomPolicy?.footer == .empty)
        #expect(room.peerMetadata(using: SingleBotRegistry(bot: makeBot())).input == .empty)
    }

    @Test(arguments: [nil, Chat.RoomFooter.textInput])
    func aRoomWithoutAnEmptyFooterKeepsTheDefaults(footer: Chat.RoomFooter?) {
        #expect(room(footer: footer).roomPolicy == policy)
    }
}

private extension ChatRoomPolicyTests {
    func makeBot() -> ProductBot {
        ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            description: nil,
            runtime: TrUAPIChatHandler(productId: "dim2.paseo", workers: StubWorkerManager()),
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }

    func room(footer: Chat.RoomFooter?) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension("dim2.paseo", roomId: "room"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomFooter: footer
        )
    }
}

/// The policy of a room takes each aspect that its product declared from the stored declarations.
struct ChatRoomPolicyDeclarationsTests {
    @Test func aProductSourceRoomKeepsItsMessagesInTheProduct() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(config: config(productSource: true)))

        #expect(policy.messageStore == .product)
        #expect(!policy.hostAppliesUserActions)
    }

    @Test func theConfigurationSetsSeenAndWhoReacted() {
        let config = Chat.RoomConfig(productSource: false, reactionDetails: true, reportSeen: true)
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(config: config))

        #expect(policy.messageStore == .host)
        #expect(policy.reportsSeen)
        #expect(policy.offersReactionDetails)
    }

    /// The footer is the one of `set_room_footer`, also for a room with declarations.
    @Test func theFooterOfTheRoomSetsTheFooter() {
        #expect(ChatRoomPolicy(footer: .textInput, declarations: declarations()).footer == .input)
        #expect(ChatRoomPolicy(footer: .empty, declarations: declarations()).footer == .empty)
    }

    @Test func theGenerationComesFromTheStore() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(generation: 3))

        #expect(policy.configGeneration == 3)
    }

    /// The list reads the policy of a stored room, so a product-source room shows as one without a worker.
    @Test func aStoredRoomReadsItsPolicy() {
        let room = Chat.LocalModel(
            peer: .chatExtension("dim2.paseo", roomId: "room"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomDeclarations: declarations(config: config(productSource: true))
        )

        #expect(room.roomPolicy?.messageStore == .product)
    }
}

private extension ChatRoomPolicyDeclarationsTests {
    func config(productSource: Bool) -> Chat.RoomConfig {
        Chat.RoomConfig(productSource: productSource, reactionDetails: false, reportSeen: false)
    }

    func declarations(
        config: Chat.RoomConfig? = nil,
        generation: Int64 = 0
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(config: config, configGeneration: generation, preview: .hostDefault)
    }
}
