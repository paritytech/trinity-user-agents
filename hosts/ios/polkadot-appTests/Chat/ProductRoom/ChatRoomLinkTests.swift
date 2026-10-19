import Foundation
import Operation_iOS
import Products
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// Links into the rooms of a product and the room events of the background (P6.9).
@MainActor
struct ChatRoomLinkTests {
    // MARK: - Link

    @Test func aRoomLinkNamesTheProductAndTheRoom() {
        #expect(ChatRoomLink("polkadot://truapi-chat-v2.dot/-/chat/source")
            == ChatRoomLink(productId: "truapi-chat-v2.dot", roomId: "source"))
        #expect(ChatRoomLink("https://Truapi-Chat-V2.DOT/-/chat/host?from=app")
            == ChatRoomLink(productId: "truapi-chat-v2.dot", roomId: "host"))
        #expect(ChatRoomLink("polkadot://game.dot/-/chat/my%20room")?.roomId == "my room")
        #expect(ChatRoomLink("polkadot://game.dot/-/chat/lobby")?.chatId == .chatExtension("game.dot", roomId: "lobby"))
    }

    @Test(arguments: [
        "polkadot://game.dot/-/chat",
        "polkadot://game.dot/-/chat/",
        "polkadot://game.dot/-/chat/a/b",
        "polkadot://game.dot/chat/lobby",
        "polkadot://game.dot/-/pocket/add?card=x",
        "https://example.com/-/chat/lobby",
        "polkadot://chat?id=1:game.dot:lobby"
    ])
    func anotherLinkIsNoRoomLink(link: String) {
        #expect(ChatRoomLink(link) == nil)
    }

    // MARK: - Pipeline

    @Test func theLinkPipelineOpensAKnownRoomAndTheAppOfAnUnknownRoom() async throws {
        let storage = UserDataStorageTestFacade()
        try await storage.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .saveOperation({ [Chat.LocalModel(
                peer: .chatExtension("game.dot", roomId: "lobby"),
                message: nil,
                unreadDisplayMessageCount: 0,
                hasIncomingReaction: false,
                createdAt: Date(),
                roomMetadata: nil
            )] }, { [] })
            .asyncExecute()
        let navigator = RecordingModuleNavigator()
        var openedProducts: [String] = []
        let service = ChatRoomLinkService(storageFacade: storage, moduleNavigator: navigator) {
            openedProducts.append($0)
        }

        #expect(try service.handle(url: #require(URL(string: "polkadot://game.dot/-/chat/lobby"))))
        #expect(try service.handle(url: #require(URL(string: "polkadot://game.dot/-/chat/unknown"))))
        #expect(try !service.handle(url: #require(URL(string: "polkadot://game.dot/page"))))

        for _ in 0 ..< 200 where navigator.openedChats.isEmpty || openedProducts.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(navigator.openedChats == [.chatExtension("game.dot", roomId: "lobby")])
        #expect(openedProducts == ["game.dot"])
    }

    /// A product opens its own rooms with `navigate_to`, and no room of another product.
    @Test func aProductCannotNavigateIntoTheRoomOfAnotherProduct() async throws {
        let bridge = await ProductRoomFixture().bridge()

        await #expect(throws: HostNavigateToError.PermissionDenied) {
            try await bridge.navigateTo(url: "polkadot://other.dot/-/chat/lobby")
        }
    }

    // MARK: - Background

    @Test func theBackgroundClosesAVisibleRoomAndTheForegroundOpensItAgain() {
        let center = NotificationCenter()
        var calls: [String] = []
        let observer = ChatRoomBackgroundObserver(
            notificationCenter: center,
            openRoom: { calls.append("open") },
            closeRoom: { calls.append("close") }
        )

        observer.screenDidAppear()
        center.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        center.post(name: UIApplication.willEnterForegroundNotification, object: nil)

        #expect(calls == ["close", "open"])
    }

    @Test func aHiddenRoomStaysClosedInTheBackground() {
        var calls: [String] = []
        let observer = ChatRoomBackgroundObserver(
            notificationCenter: NotificationCenter(),
            openRoom: { calls.append("open") },
            closeRoom: { calls.append("close") }
        )

        observer.enterBackground()
        observer.screenDidAppear()
        observer.enterForeground()
        observer.screenDidDisappear()
        observer.enterBackground()

        #expect(calls.isEmpty)
    }
}

@MainActor
final class RecordingModuleNavigator: ModuleNavigating {
    private(set) var openedChats: [Chat.Id] = []

    func openChat(_ model: ChatOpenModel) {
        if case let .existingChat(chatId) = model {
            openedChats.append(chatId)
        }
    }

    func presentModally(_: UIViewController) {}
    func presentFullScreen(_: UIViewController, transitioningDelegate _: UIViewControllerTransitioningDelegate?) {}
    func openProduct(page _: ProductPage) {}
    func openPocket() {}
    func openScanPanel() {}
}
