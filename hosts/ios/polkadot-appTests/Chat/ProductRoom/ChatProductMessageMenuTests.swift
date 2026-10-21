import Foundation
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The message menu of a product room (P6.1, scenarios S25 to S28).
@MainActor
struct ChatProductMessageMenuTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let hostActions: [ChatMessageBubbleAction] = [.reply, .copy, .edit, .delete]

    // MARK: - Entries

    @Test func noAnswerKeepsTheHostEntries() {
        #expect(ChatMessageBubbleAction.productMenu(hostActions: hostActions, answer: nil) == hostActions)
    }

    /// S25: the defaults minus Edit, then the product entries.
    @Test func extendBuiltInDropsTheHiddenEntriesAndAddsTheProductEntries() {
        let pin = Data.action("act-pin")

        let entries = ChatMessageBubbleAction.productMenu(
            hostActions: hostActions,
            answer: .extendBuiltIn([pin], hidden: [.edit])
        )

        #expect(entries == [.reply, .copy, .delete, .product(pin)])
    }

    /// S26: only the product entries, a destructive entry last.
    @Test func customGivesOnlyTheProductEntriesWithTheDestructiveEntryLast() {
        let report = Data.action("act-report", destructive: true)
        let pin = Data.action("act-pin")

        let entries = ChatMessageBubbleAction.productMenu(hostActions: hostActions, answer: .custom([report, pin]))

        #expect(entries == [.product(pin), .product(report)])
    }

    @Test func theMenuDrawsTheProductEntriesInASectionOfTheirOwn() {
        let entries: [ChatMessageBubbleAction] = [.copy, .product(Data.action("act-pin"))]

        let elements = ChatViewModelFactory.menuElements(
            entries,
            messageId: "m1",
            content: ChatMessageMenuContent(text: "hello", actions: .inert),
            icons: [:],
            productRoom: nil
        )

        #expect(elements.count == 2)
        let section = elements.last as? UIMenu
        #expect(section?.options.contains(.displayInline) == true)
        #expect((section?.children.first as? UIAction)?.title == "act-pin")
    }

    /// A product-source room has no host copy of the versions of an edited message, so the host entry "View edit
    /// history" is not in its menu. The product adds its own entry.
    @Test func aProductSourceRoomHasNoHostEditHistory() throws {
        let presenter = try makePresenter(productSource: true)
        let history = Data.action("edit-history")

        let elements = ChatViewModelFactory.menuElements(
            [.copy, .edited, .product(history)],
            messageId: "m1",
            content: ChatMessageMenuContent(text: "hello", actions: .inert),
            icons: [:],
            productRoom: presenter
        )

        #expect(!presenter.offersHostEditHistory)
        #expect(elements.count == 2)
        #expect((elements.first as? UIAction)?.title == String(localized: .chatCopy))
        let section = elements.last as? UIMenu
        #expect((section?.children.first as? UIAction)?.title == "edit-history")
    }

    @Test func aHostStoreRoomKeepsTheHostEditHistory() throws {
        let presenter = try makePresenter()

        let elements = ChatViewModelFactory.menuElements(
            [.copy, .edited],
            messageId: "m1",
            content: ChatMessageMenuContent(text: "hello", actions: .inert),
            icons: [:],
            productRoom: presenter
        )

        #expect(presenter.offersHostEditHistory)
        #expect((elements.last as? UIAction)?.title == String(localized: .chatEditHistory))
    }

    @Test func aDestructiveProductEntryDrawsInRed() {
        let action = UIAction.chatProduct(Data.action("act-report", destructive: true), image: nil) {}

        #expect(action.attributes.contains(.destructive))
    }

    // MARK: - Question

    @Test func aRoomThatDoesNotDeclareMenusAsksNothing() async throws {
        let presenter = try makePresenter(messageActions: false)

        #expect(await presenter.messageMenu(for: "m1") == nil)
        #expect(room.calls.messageMenus.isEmpty)
    }

    @Test func theQuestionCarriesTheMessageWithTheIdOfTheProductAndNoCustomPayload() async throws {
        let stored = Chat.ProductMessageId.stored("card-1", in: Data.chatId)
        let custom = Chat.LocalMessage.Content.customRendered(.init(
            decoderId: MessageDecoderIdentifier.product.rawValue,
            data: Foundation.Data([1, 2, 3]),
            identifier: "chat-v2-card",
            alt: "Sample card"
        ))
        let presenter = try makePresenter(messages: [Data.message(stored, content: custom)])
        room.messageMenuAnswer = .success(.init(menu: .custom(actions: [Data.wireAction("act-pin")])))

        let answer = await presenter.messageMenu(for: stored)

        #expect(answer == .custom([Data.action("act-pin")]))
        let request = try #require(room.calls.messageMenus.first)
        #expect(request.roomId == "lobby")
        #expect(request.message.messageId == "card-1")
        #expect(request.message.origin.participantId == "product")
        #expect(request.message.body == .custom(
            messageType: "chat-v2-card",
            payload: Foundation.Data(),
            previewText: "Sample card"
        ))
    }

    /// S26: a refusal, a failure or a time-out keeps the default menu.
    @Test(arguments: [
        ChatQueryError.refused(.notAvailable),
        ChatQueryError.timedOut,
        ChatQueryError.noAnswer
    ])
    func noAnswerGivesTheDefaultMenu(error: ChatQueryError) async throws {
        let presenter = try makePresenter()
        room.messageMenuAnswer = .failure(error)

        #expect(await presenter.messageMenu(for: "m1") == nil)
        #expect(room.calls.messageMenus.count == 1)
    }

    @Test func theAnswerMapsHiddenBuiltIns() async throws {
        let presenter = try makePresenter()
        room.messageMenuAnswer = .success(.init(menu: .extendBuiltIn(
            actions: [Data.wireAction("act-pin")],
            hidden: [.edit, .react]
        )))

        let answer = await presenter.messageMenu(for: "m1")

        #expect(answer == .extendBuiltIn([Data.action("act-pin")], hidden: [.edit, .react]))
    }

    // MARK: - Selection

    /// S28: a callback entry sends `MessageActionTriggered` with the id of the product.
    @Test func aCallbackEntrySendsMessageActionTriggered() async throws {
        let stored = Chat.ProductMessageId.stored("host-p-0001", in: Data.chatId)
        let presenter = try makePresenter(messages: [Data.message(stored)])

        presenter.select(Data.action("act-pin"), messageId: stored)
        await room.waitForEvents(1)

        #expect(room.events == [.messageActionTriggered(messageId: "host-p-0001", actionId: "act-pin")])
        #expect(wireframe.confirmed.isEmpty)
    }

    /// S27: a link entry asks the user, then opens the link. The product receives no event.
    @Test func aLinkEntryOpensTheLinkAfterTheUserConfirms() async throws {
        let presenter = try makePresenter()
        let link = "polkadot://truapi-chat-v2.dot/?from=deeplink"

        presenter.select(Data.action("act-open-app", handler: .deeplink(url: link)), messageId: "m1")

        #expect(wireframe.confirmed == [URL(string: link)])
        #expect(wireframe.opened == [URL(string: link)])
        try await Task.sleep(for: .milliseconds(20))
        #expect(room.events.isEmpty)
    }

    @Test func aLinkThatTheUserDoesNotConfirmStaysClosed() throws {
        let presenter = try makePresenter()
        wireframe.confirms = false

        presenter.select(Data.action("act-open-app", handler: .deeplink(url: "https://example.com")), messageId: "m1")

        #expect(wireframe.confirmed.count == 1)
        #expect(wireframe.opened.isEmpty)
    }

    @Test func aLinkOfTheAppGoesThroughTheLinkPipelineAndAWebLinkToTheBrowser() throws {
        #expect(try ChatProductLinkOpener.route(for: #require(URL(string: "polkadot://a.dot/-/chat/x"))) == .app)
        #expect(try ChatProductLinkOpener.route(for: #require(URL(string: "https://example.com"))) == .web)
        #expect(try ChatProductLinkOpener.route(for: #require(URL(string: "mailto:a@b.c"))) == .system)
    }
}

private extension ChatProductMessageMenuTests {
    func makePresenter(
        messageActions: Bool = true,
        productSource: Bool = false,
        messages: [Chat.LocalMessage] = [ProductRoomTestData.message("m1")]
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: ProductRoomTestData.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.update(
            policy: ProductRoomTestData.policy(ProductRoomTestData.config(
                messageActions: messageActions,
                productSource: productSource
            )),
            messages: messages
        )
        return presenter
    }
}

extension ChatViewModelActions {
    /// Actions that do nothing, for a test that draws a menu.
    static var inert: ChatViewModelActions {
        ChatViewModelActions(
            reply: { _ in },
            copy: { _ in },
            edit: { _ in },
            toggleReaction: { _, _ in },
            showReactionDetails: { _ in },
            acceptChatRequest: {},
            declineChatRequest: {},
            showEditHistory: { _ in },
            showFile: { _ in },
            openLink: { _ in },
            processAction: { _ in },
            selectAttachment: { _ in },
            unblockUser: {},
            startCall: { _ in },
            openProduct: { _ in },
            retry: { _ in },
            delete: { _ in }
        )
    }
}
