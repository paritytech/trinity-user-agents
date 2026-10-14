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

    /// Only a declared name is a command, so a room that declares none takes every `/text` as a message.
    @Test func noNameIsACommand() {
        #expect(policy.slashCommands == .declared([]))
        #expect(!policy.slashCommands.isCommand("help"))
    }

    @Test func theProductIsAskedNothing() {
        #expect(policy.productQueries.isEmpty)
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
        #expect(policy.offersRetry)
    }

    @Test func theConfigurationSetsTheMenuThePlaceholderAndSeen() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(config: Chat.RoomConfig(
            productSource: false,
            reactionDetails: true,
            participants: true,
            messageActions: true,
            participantActions: false,
            allowedReactions: ["👍", "🎉"],
            placeholder: "Ask the bot",
            reportSeen: true
        )))

        #expect(policy.messageStore == .host)
        #expect(policy.reportsSeen)
        #expect(policy.offersReactionDetails)
        #expect(policy.messageMenu.allowedReactions == ["👍", "🎉"])
        #expect(policy.placeholder == "Ask the bot")
        #expect(policy.productQueries == [.participants, .messageActions])
        #expect(!policy.offersRetry)
    }

    @Test func onlyTheDeclaredCommandsAreCommands() {
        let commands = [Chat.RoomCommand(name: "roll", description: "Roll dice", argumentHint: nil)]
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(commands: commands))

        #expect(policy.slashCommands == .declared(["roll"]))
        #expect(policy.slashCommands.isCommand("roll"))
        #expect(!policy.slashCommands.isCommand("help"))
    }

    @Test func theDeclaredInputSetsTheFooter() {
        let input = Chat.RoomInputConfig(placeholder: "Type", photo: false, video: true, file: true, payment: nil)
        let policy = ChatRoomPolicy(footer: .textInput, declarations: declarations(footer: .input(input)))

        #expect(policy.footer == .input(ChatRoomPolicy.Input(
            allowsPhoto: false,
            allowsVideo: true,
            allowsFile: true,
            allowsPayment: false,
            placeholder: "Type"
        )))
        #expect(ChatRoomPolicy(footer: .textInput, declarations: declarations(footer: .empty)).footer == .empty)
    }

    @Test func theSuggestionsAndTheGenerationComeFromTheStore() {
        let suggestions = [Chat.RoomSuggestion(suggestionId: "a", text: "Yes", sendsText: true)]
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(suggestions: suggestions, generation: 3))

        #expect(policy.suggestions == ["Yes"])
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
        Chat.RoomConfig(
            productSource: productSource,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
    }

    func declarations(
        config: Chat.RoomConfig? = nil,
        footer: Chat.RoomFooterConfig? = nil,
        suggestions: [Chat.RoomSuggestion]? = nil,
        commands: [Chat.RoomCommand]? = nil,
        generation: Int64 = 0
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: config,
            configGeneration: generation,
            preview: .hostDefault,
            toolbar: nil,
            suggestions: suggestions,
            commands: commands,
            footerConfig: footer
        )
    }
}
