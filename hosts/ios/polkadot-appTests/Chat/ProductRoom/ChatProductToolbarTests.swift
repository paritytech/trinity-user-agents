import Foundation
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The toolbar of a product room (P6.2, scenario S29).
@MainActor
struct ChatProductToolbarTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()

    @Test func theBarShowsTwoButtonsAndTheOthersGoInTheOverflowMenu() {
        let (inline, overflow) = ChatToolbarLayout.split(["tb-1", "tb-2", "tb-3", "tb-4"])

        #expect(inline == ["tb-1", "tb-2"])
        #expect(overflow == ["tb-3", "tb-4"])
    }

    @Test func aDeclaredToolbarDrawsTheOverflowButtonThenTheInlineButtons() throws {
        let presenter = try makePresenter(toolbar: (1 ... 4).map { button("tb-\($0)") })

        let items = try #require(view.toolbars.last)
        #expect(items.count == 3)
        #expect(items[0].menu?.children.map { ($0 as? UIAction)?.title } == ["tb-3", "tb-4"])
        #expect(items[1].primaryAction?.title == "tb-2")
        #expect(items[2].primaryAction?.title == "tb-1")
        _ = presenter
    }

    /// P7.2: VoiceOver reads the label of each toolbar button, and names the overflow button.
    @Test func eachToolbarButtonHasALabelForVoiceOver() throws {
        _ = try makePresenter(toolbar: (1 ... 4).map { button("tb-\($0)") })

        let items = try #require(view.toolbars.last)
        #expect(items[0].accessibilityLabel == String(localized: .ChatExtension.toolbarMore))
        #expect(items[1].accessibilityLabel == "tb-2")
        #expect(items[2].accessibilityLabel == "tb-1")
    }

    @Test func twoButtonsNeedNoOverflowButton() throws {
        _ = try makePresenter(toolbar: [button("tb-1"), button("tb-2")])

        #expect(view.toolbars.last?.count == 2)
        #expect(view.toolbars.last?.contains { $0.menu != nil } == false)
    }

    @Test func aRoomWithoutToolbarShowsNoButton() throws {
        _ = try makePresenter(toolbar: nil)

        #expect(view.toolbars.last?.isEmpty == true)
    }

    @Test func aCallbackButtonSendsToolbarActionTriggered() async throws {
        let presenter = try makePresenter(toolbar: [button("tb-2")])

        presenter.pressToolbar(button("tb-2"))
        await room.waitForEvents(1)

        #expect(room.events == [.toolbarActionTriggered(actionId: "tb-2")])
    }

    @Test func aLinkButtonOpensTheLinkAfterTheUserConfirms() throws {
        let link = "polkadot://truapi-chat-v2.dot/?from=deeplink"
        let presenter = try makePresenter(toolbar: [button("tb-1", handler: .deeplink(url: link))])

        presenter.pressToolbar(button("tb-1", handler: .deeplink(url: link)))

        #expect(wireframe.opened == [URL(string: link)])
        #expect(room.calls.events.isEmpty)
    }
}

private extension ChatProductToolbarTests {
    func button(_ id: String, handler: Chat.ProductActionHandler = .callback) -> Chat.RoomToolbarAction {
        Chat.RoomToolbarAction(
            actionId: id,
            label: id,
            icon: .inline(mimeType: "image/png", data: Foundation.Data()),
            handler: handler
        )
    }

    func makePresenter(toolbar: [Chat.RoomToolbarAction]?) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.update(
            policy: ChatRoomPolicy(footer: nil, declarations: Data.declarations(toolbar: toolbar)),
            messages: []
        )
        return presenter
    }
}
