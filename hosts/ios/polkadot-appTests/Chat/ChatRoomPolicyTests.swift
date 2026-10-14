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

    @Test func theListShowsTheLastMessageAndTheHostUnreadCount() {
        #expect(policy.listPreview == .lastMessage)
        #expect(policy.listBadge == .hostUnreadCount)
        #expect(policy.listBadgeCount(hostUnreadCount: 3) == 3)
    }

    @Test func seenMessagesAreNotReported() {
        #expect(!policy.reportsSeen)
    }

    @Test func theMessageMenuHasEveryBuiltInAndAllEmoji() {
        #expect(policy.messageMenu.builtIns == [.copy, .reply, .edit, .delete, .react, .retry])
        #expect(policy.messageMenu.allowedReactions == nil)
    }

    @Test func theParticipantsAreTheUserAndTheProduct() {
        #expect(policy.participants == [.user, .product])
    }

    @Test func whoReactedIsNotOffered() {
        #expect(!policy.offersReactionDetails)
    }

    @Test func thereIsNoToolbar() {
        #expect(policy.toolbar.isEmpty)
        #expect(makeBot().peerMetadata.moreActions.isEmpty)
    }

    @Test func theFooterIsTheInputWithPhotoAndVideoOnly() {
        let expected = ChatRoomPolicy.Input(
            allowsPhoto: true,
            allowsVideo: true,
            allowsFile: false,
            allowsPayment: false,
            placeholder: nil
        )

        #expect(policy.footer == .input(expected))
    }

    /// Until P6.7 a product room cannot take an attachment, so the input shows no attachment button.
    @Test func theInputShowsNoPaymentAndNoAttachmentButton() {
        let expected = Chat.PeerMetadataInput.inputField(.init(canPay: false, canAttachFile: false))

        #expect(policy.peerInput == expected)
        #expect(makeBot().peerMetadata.input == expected)
    }

    @Test func anEmptyFooterHidesTheInput() {
        let room = room(footer: .empty)

        #expect(room.roomPolicy?.footer == .empty)
        #expect(room.roomPolicy?.peerInput == .empty)
    }

    @Test(arguments: [nil, Chat.RoomFooter.textInput])
    func aRoomWithoutAnEmptyFooterKeepsTheDefaults(footer: Chat.RoomFooter?) {
        #expect(room(footer: footer).roomPolicy == policy)
    }

    @Test func theEmptyRoomTextIsTheHostText() {
        #expect(policy.placeholder == nil)
    }

    @Test func thereAreNoSuggestionsAndNoActivity() {
        #expect(policy.suggestions.isEmpty)
        #expect(policy.activity == nil)
    }

    /// No product can declare its commands yet, so any `/name` is a command.
    @Test func anyNameIsACommand() {
        #expect(policy.slashCommands == .anyName)
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
