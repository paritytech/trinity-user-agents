import ExternalAccessibility
import Foundation
import os
import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The footer of a product room (P6.6, scenarios S33 to S35).
@MainActor
struct ChatProductFooterTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()

    /// S33: the hint and the attachment toggles of `Input`. No payment path exists in a product room yet.
    @Test func anInputFooterHonoursThePlaceholderAndTheToggles() {
        let none = ChatRoomPolicy.Input(
            allowsPhoto: false,
            allowsVideo: false,
            allowsFile: false,
            allowsPayment: true,
            placeholder: "Message the sample"
        )

        #expect(ChatProductRoomPresenter.inputViewConfiguration(none) == ChatInputViewConfiguration(
            placeholder: "Message the sample",
            maxNumberOfLines: 7,
            maxCharacterCount: ChatProductRoomPresenter.inputCharacterLimit,
            canPay: false,
            canAttachFile: false,
            canSendWithoutText: false,
            inputAccessibilityId: AccessibilityID.Chat.messageInput,
            sendAccessibilityId: AccessibilityID.Chat.sendButton
        ))
        #expect(none.attachmentKinds.isEmpty)

        let all = ChatRoomPolicy.Input(
            allowsPhoto: true,
            allowsVideo: true,
            allowsFile: true,
            allowsPayment: false,
            placeholder: nil
        )
        #expect(all.attachmentKinds == [.photo, .video, .file])
    }

    @Test func eachFooterKindGivesItsBody() throws {
        let input = try makePresenter(footer: .input(.init(
            placeholder: "Hi",
            photo: true,
            video: false,
            file: false,
            payment: nil
        )))
        let empty = try makePresenter(footer: .empty)
        let custom = try makePresenter(footer: .custom)

        guard case .input? = (input.inputConfiguration() as? ChatProductInputConfiguration)?.body else {
            Issue.record("an Input footer must draw the input")
            return
        }
        guard case .empty? = (empty.inputConfiguration() as? ChatProductInputConfiguration)?.body else {
            Issue.record("an Empty footer must draw no input")
            return
        }
        guard case .custom? = (custom.inputConfiguration() as? ChatProductInputConfiguration)?.body else {
            Issue.record("a Custom footer must draw the tree of the product")
            return
        }
    }

    @Test func aCustomFooterIsAFooterOfThePolicy() {
        let policy = ChatRoomPolicy(footer: nil, declarations: Data.declarations(footer: .custom))

        #expect(policy.footer == .custom)
        #expect(policy.peerInput == .empty)
        #expect(!policy.footer.showsInput)
    }

    @Test func theSameFooterKeepsItsViewAndAnotherHintRebuildsIt() throws {
        let accessory = ChatRoomAccessoryModel()
        let hint = ChatInputViewConfiguration.chat(canPay: false, canAttachFile: false)
        let first = ChatProductInputConfiguration(body: .input(hint, onAttach: nil), accessory: accessory)
        let same = ChatProductInputConfiguration(body: .input(hint, onAttach: {}), accessory: accessory)
        let empty = ChatProductInputConfiguration(body: .empty, accessory: accessory)
        let otherModel = ChatProductInputConfiguration(body: .input(hint, onAttach: nil), accessory: .init())

        #expect(first.equalsTo(configuration: same))
        #expect(!first.equalsTo(configuration: empty))
        #expect(!first.equalsTo(configuration: otherModel))
    }

    /// S35: the footer tree comes from the renderer, and a press goes back to it with the room.
    @Test func theCustomFooterDrawsTheTreeOfTheProduct() async throws {
        let renderer = RecordingFooterRenderer()
        let widget = ProductFooterWidgetViewModel(roomId: "lobby", renderer: renderer, logger: StubLogger())

        for _ in 0 ..< 200 where widget.node == nil {
            try await Task.sleep(for: .milliseconds(5))
        }

        #expect(widget.node != nil)
        #expect(renderer.rooms == ["lobby"])
    }
}

private final class RecordingFooterRenderer: ProductRoomFooterRendering, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: [String]())

    var rooms: [String] {
        state.withLock { $0 }
    }

    func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        state.withLock { $0.append(roomId) }
        return AsyncThrowingStream { continuation in
            continuation.yield(.native(.text(
                modifiers: [],
                props: TextProps(style: nil, color: nil),
                children: [.string(text: "footer")]
            )))
        }
    }

    func dispatchFooterAction(roomId _: String, actionId _: String, payload _: String?) async {}
}

private extension ChatProductFooterTests {
    func makePresenter(footer: Chat.RoomFooterConfig) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.update(
            policy: ChatRoomPolicy(footer: nil, declarations: Data.declarations(footer: footer)),
            messages: []
        )
        return presenter
    }
}
