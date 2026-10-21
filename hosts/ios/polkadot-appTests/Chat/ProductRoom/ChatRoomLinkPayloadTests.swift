import AVFoundation
import Foundation
import Operation_iOS
import os
import Testing
import TrUAPIHost
import UIKit
import UIKitExt
@testable import polkadot_app

/// A room link can carry a payload for its product (`chat.openLink`): the link of a contact, for example. The host
/// reads the room from the link, gives the rest to the product, and reads nothing in it.
@MainActor
struct ChatRoomLinkPayloadTests {
    private static let contactLink = "verbosity:/contact#AAEC-_z9"

    // MARK: - Link

    @Test func theTextAfterTheHashIsThePayloadPercentDecoded() {
        let link = ChatRoomLink("polkadot://verbosity.dot/-/chat/verbosity#verbosity:/contact%23AAEC-_z9")

        #expect(link == ChatRoomLink(productId: "verbosity.dot", roomId: "verbosity", payload: Self.contactLink))
        #expect(ChatRoomLink("polkadot://verbosity.dot/-/chat/verbosity")?.payload == nil)
        #expect(ChatRoomLink("polkadot://verbosity.dot/-/chat/verbosity#")?.payload == nil)
    }

    @Test(arguments: [
        "https://verbosity.dot.li/-/chat/verbosity#verbosity:/contact%23AAEC-_z9",
        "https://Verbosity.DOT.li/-/chat/verbosity#verbosity:/contact%23AAEC-_z9",
        "polkadotappdev://verbosity.dot/-/chat/verbosity#verbosity:/contact%23AAEC-_z9",
        "polkadotapp://verbosity.dot/-/chat/verbosity?from=qr#verbosity:/contact%23AAEC-_z9"
    ])
    func theWebFormAndTheAppSchemeNameTheSameRoom(link: String) {
        #expect(ChatRoomLink(link)
            == ChatRoomLink(productId: "verbosity.dot", roomId: "verbosity", payload: Self.contactLink))
    }

    @Test(arguments: [
        "https://dot.li/-/chat/verbosity#x",
        "https://verbosity.example.li/-/chat/verbosity#x",
        "https://verbosity.dot.li/verbosity#x",
        "ftp://verbosity.dot.li/-/chat/verbosity#x"
    ])
    func aWebLinkOfNoProductRoomIsNoRoomLink(link: String) {
        #expect(ChatRoomLink(link) == nil)
    }

    // MARK: - Pipeline

    @Test func aLinkWithAPayloadOpensItsRoomAndGivesThePayloadToTheProduct() async throws {
        let storage = UserDataStorageTestFacade()
        try await storage.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .saveOperation({ [Chat.LocalModel(
                peer: .chatExtension("verbosity.dot", roomId: "verbosity"),
                message: nil,
                unreadDisplayMessageCount: 0,
                hasIncomingReaction: false,
                createdAt: Date(),
                roomMetadata: nil
            )] }, { [] })
            .asyncExecute()
        let navigator = RecordingModuleNavigator()
        let opener = RecordingLinkOpener()
        let service = ChatRoomLinkService(storageFacade: storage, moduleNavigator: navigator, linkOpener: opener)

        let withPayload = "https://verbosity.dot.li/-/chat/verbosity#verbosity:/contact%23AAEC-_z9"
        #expect(try service.handle(url: #require(URL(string: withPayload))))
        #expect(try service.handle(url: #require(URL(string: "polkadot://verbosity.dot/-/chat/verbosity"))))

        for _ in 0 ..< 200 where navigator.openedChats.count < 2 || opener.links.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(navigator.openedChats == Array(
            repeating: .chatExtension("verbosity.dot", roomId: "verbosity"),
            count: 2
        ))
        #expect(opener.links == [ChatRoomLink(withPayload)])
    }

    @Test func theProductTakesThePayloadAndTheHostShowsTheRoomOfItsAnswer() async {
        let bot = LinkTakingBot(answer: ProductChatOpenLinkItem(roomId: "dm-7"))
        let navigator = RecordingModuleNavigator()
        let opener = ProductRoomLinkOpener(registry: SingleBotRegistry(bot: bot), moduleNavigator: navigator)

        await opener.open(ChatRoomLink(productId: "game.dot", roomId: "lobby", payload: Self.contactLink))

        #expect(bot.requests == [ProductChatOpenLinkRequest(roomId: "lobby", payload: Self.contactLink)])
        #expect(navigator.openedChats == [.chatExtension("game.dot", roomId: "dm-7")])
    }

    @Test func anAnswerForTheSameRoomOrARefusalKeepsTheRoom() async {
        let navigator = RecordingModuleNavigator()
        for bot in [
            LinkTakingBot(answer: ProductChatOpenLinkItem(roomId: nil)),
            LinkTakingBot(answer: ProductChatOpenLinkItem(roomId: "lobby")),
            LinkTakingBot(answer: nil)
        ] {
            let opener = ProductRoomLinkOpener(
                registry: SingleBotRegistry(bot: bot),
                moduleNavigator: navigator,
                logger: StubLogger()
            )
            await opener.open(ChatRoomLink(productId: "game.dot", roomId: "lobby", payload: "x"))
            #expect(bot.requests.count == 1)
        }
        #expect(navigator.openedChats.isEmpty)
    }

    // MARK: - Scan

    /// A camera cannot scan on the simulator, so the scanner gets the text of the code here.
    @Test func theMainScannerGivesARoomLinkOfAProductToTheLinkPipeline() {
        let delegate = RecordingScanDelegate()
        let presenter = WalletQRScanPresenter(
            matcher: AddressQRMatcher(),
            wireframe: QRScannerWireframe(),
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            delegate: delegate,
            dsfinvkParser: W3sDsfinvkReceiptParser(),
            isAcceptedScheme: { $0 == "polkadotapp" },
            qrScanService: QRCaptureService(delegate: nil, delegateQueue: QRCaptureService.processingQueue),
            qrExtractionService: QRExtractionService(processingQueue: QRCaptureService.processingQueue)
        )
        let view = UncoveredScannerView()
        presenter.view = view

        let code = "https://verbosity.dot.li/-/chat/verbosity#verbosity:/contact%23AAEC-_z9"
        presenter.handle(code: code)

        #expect(delegate.urls == [URL(string: code)])
    }

    @Test func aScannedRoomLinkStaysInTheAppAndGoesToItsLinkHandler() async throws {
        let links = RecordingDeferredLinks()
        let handler = WalletQRScanResultHandler(dsfinvkRouter: UnusedDsfinvkRouter(), linkHandler: links)
        let url = try #require(URL(string: "https://verbosity.dot.li/-/chat/verbosity#verbosity:/contact%23AAEC"))

        handler.walletQRScanDidReceiveURL(url)

        #expect(links.urls == [url])
    }
}

// MARK: - Doubles

private final class RecordingLinkOpener: ProductRoomLinkOpening, @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock<[ChatRoomLink]>(initialState: [])

    var links: [ChatRoomLink] {
        lock.withLock { $0 }
    }

    func open(_ link: ChatRoomLink) async {
        lock.withLock { $0.append(link) }
    }
}

/// A product bot that takes links with a fixed answer, or refuses them when the answer is `nil`.
private final class LinkTakingBot: ChatExtensionBotProtocol, ProductChatLinkTaking, @unchecked Sendable {
    let identifier: ChatExtension.Id = "game.dot"
    private let answer: ProductChatOpenLinkItem?
    private let lock = OSAllocatedUnfairLock<[ProductChatOpenLinkRequest]>(initialState: [])

    init(answer: ProductChatOpenLinkItem?) {
        self.answer = answer
    }

    var requests: [ProductChatOpenLinkRequest] {
        lock.withLock { $0 }
    }

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(name: "Game", icon: .product(domain: identifier), input: .empty, moreActions: [])
    }

    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem {
        lock.withLock { $0.append(request) }
        guard let answer else { throw ChatQueryError.refused(.notAvailable) }
        return answer
    }

    func deliverAutomaticMessages(_: ChatExtensionDiscoverContextProtocol) {}

    func attach(presentationView _: ControllerBackedProtocol) {}

    func process(
        message _: Chat.LocalMessage,
        lastProcessingOutcome _: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        .skipped
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {}
}

private final class RecordingScanDelegate: WalletQRScanDelegate {
    private(set) var urls: [URL] = []

    func walletQRScanDidReceiveURL(_ url: URL) {
        urls.append(url)
    }

    func walletQRScanDidReceiveDsfinvkReceipt(_: W3sDsfinvkReceipt) {}
}

private final class UncoveredScannerView: UIViewController, QRScannerViewProtocol {
    var isCoveredByModal: Bool { false }

    func didReceive(session _: AVCaptureSession) {}

    func present(message _: String, animated _: Bool, autoDismiss _: Bool) {}
}

private final class RecordingDeferredLinks: DeferredLinkHandling {
    private(set) var urls: [URL] = []

    func handle(with url: URL) {
        urls.append(url)
    }

    func register(_: URLHandlingServiceProtocol) {}
}

private final class UnusedDsfinvkRouter: W3sDsfinvkRouting {
    func route(_: W3sDsfinvkReceipt) async {}
}
