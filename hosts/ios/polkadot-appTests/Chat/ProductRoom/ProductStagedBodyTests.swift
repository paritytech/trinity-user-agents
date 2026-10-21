import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The bodies of staged attachments, and the user events that they make (P10.3).
struct ProductStagedBodyTests {
    private static let key = Foundation.Data(repeating: 7, count: 32)

    @Test func aStagedSourceGoesToTheProductAsAPreimageWithItsFacts() {
        let registry = ProductStagedMediaRegistry()
        let entry = ProductOutboxEntry(
            id: "s",
            productId: "game.dot",
            roomId: "lobby",
            store: .host,
            kind: .image,
            mimeType: "image/png",
            sizeBytes: 3_000_000,
            width: 1_200,
            height: 800,
            placeholder: "LEHV6nWB2yk8",
            createdAt: 1,
            key: Self.key,
            state: .sending
        )
        registry.register(key: Self.key, meta: ProductStagedMedia(entry))
        let body = Chat.LocalMessage.Content.ProductBody.richText(.init(
            text: nil,
            media: [polkadot_app.ChatResource.preimageSource(key: Self.key)]
        ))

        guard case let .richText(nil, media)? = body.userWireBody(staged: registry),
              case let .image(.preimage(key), meta)? = media.first else {
            Issue.record("a staged image goes as a preimage")
            return
        }
        #expect(key == Self.key)
        #expect(meta.mimeType == "image/png")
        #expect(meta.sizeBytes == 3_000_000)
        #expect(meta.width == 1_200)
    }

    /// The stager sends a staged attachment itself, so the bot sends no event for its message.
    @Test(arguments: [
        (Chat.LocalMessage.Status.outgoing(.new), false),
        (.outgoing(.failed), false),
        (.outgoing(.delivered), true)
    ])
    func aStagedMessageIsNoUserEvent(status: Chat.LocalMessage.Status, staged: Bool) {
        let media = staged ? polkadot_app.ChatResource.preimageSource(key: Self.key) : "data:image/png;base64,AA=="
        let message = ProductRoomTestData.message(
            "u1",
            content: .productBody(.richText(.init(text: nil, media: [media]))),
            origin: .user,
            status: status
        )

        #expect(ProductChatUserEvent(userMessage: message) == nil)
    }
}
