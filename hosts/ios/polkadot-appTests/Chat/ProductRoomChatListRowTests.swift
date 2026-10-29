import ChainRegistry
import Foundation
@testable import PolkadotUI
import Testing
@testable import polkadot_app

/// The chat list draws the activity line and the image badge that a product sets in the preview of its room.
struct ProductRoomChatListRowTests {
    /// The activity line replaces the preview line, and an ongoing activity animates the line.
    @Test(arguments: [true, false])
    func anActivityReplacesThePreviewLine(ongoing: Bool) throws {
        let activity = Chat.RoomActivity(text: "Ben is typing", ongoing: ongoing)

        let row = try row(for: room(preview: preview(activity: activity)))

        #expect(row.message == "Ben is typing")
        #expect(row.isMessageOngoing == ongoing)
    }

    @Test func withoutAnActivityTheRowShowsTheLastMessage() throws {
        let row = try row(for: room(preview: preview()))

        #expect(row.message == "Your turn")
        #expect(!row.isMessageOngoing)
    }

    /// The row loads an inline image or an `https` image. Any other icon shows no badge.
    @Test(arguments: [
        (pngIcon, true),
        ("https://example.com/badge.png", true),
        ("http://example.com/badge.png", false)
    ])
    func anImageBadgeShowsItsImage(icon: String, showsImage: Bool) throws {
        let row = try row(for: room(preview: preview(badge: .image(icon))))

        #expect(row.badgeImage?.key == (showsImage ? icon : nil))
    }

    @Test func aCountBadgeShowsNoImage() throws {
        let row = try row(for: room(preview: preview(badge: .unreadCount(2))))

        #expect(row.badgeImage == nil)
    }
}

private let pngIcon = "data:image/png;base64," + Data([0x89, 0x50, 0x4E, 0x47]).base64EncodedString()

private extension ProductRoomChatListRowTests {
    func preview(
        badge: Chat.RoomBadge = .hostDefault,
        activity: Chat.RoomActivity? = nil
    ) -> Chat.RoomPreview {
        Chat.RoomPreview(
            timestamp: nil,
            badge: badge,
            lastMessage: nil,
            unseenReaction: false,
            pinned: false,
            activity: activity
        )
    }

    func room(preview: Chat.RoomPreview) -> Chat.LocalModel {
        let chatId = Chat.Id.chatExtension("game.dot", roomId: "lobby")

        return Chat.LocalModel(
            peer: .chatExtension("game.dot", roomId: "lobby"),
            message: Chat.LocalMessage(
                messageId: "m1",
                chatId: chatId,
                origin: .chatExtension("game.dot"),
                creationSource: .localDevice,
                status: .incoming(.seen),
                timestamp: 1_000,
                content: .text("Your turn"),
                reactions: [],
                compactionId: nil,
                relatedMessages: []
            ),
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomDeclarations: Chat.RoomDeclarations(
                config: nil,
                configGeneration: 0,
                preview: preview,
                placeholder: nil
            )
        )
    }

    func row(for chat: Chat.LocalModel) throws -> DSChatListItemConfiguration {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: NoChatMessageDecoderFactory(),
            productIconViewModelFactory: NoProductIconViewModelFactory(),
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )
        let peerMetadata = Chat.PeerListMetadata(name: "Lobby", icon: .bot)

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: [ChatWithPeerMetadata(chat: chat, peerMetadata: peerMetadata)],
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        return try #require(viewModel.contactsById.first).configuration
    }
}

private struct NoChatMessageDecoderFactory: ChatMessageDecoderMaking {
    func makeDecoders(for _: ChainModel, chatId _: Chat.Id) -> [ChatMessageCustomDecoding] {
        []
    }
}

private struct NoProductIconViewModelFactory: ProductIconViewModelMaking {
    func createViewModel(for _: String) -> any ImageViewModelProtocol {
        StaticImageViewModel(image: nil)
    }
}
