import ChainRegistry
import Foundation
@testable import PolkadotUI
import Testing
@testable import polkadot_app

/// The room policy carries the features and the user state of the Chat Extension v2 additions, so each later item
/// reads one value (07-LATER §4.4).
struct ChatRoomAdditionsPolicyTests {
    @Test func aRoomThatDeclaresNothingHasTheDefaults() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations())

        #expect(policy.features == .hostDefault)
        #expect(policy.userState == .default)
        #expect(ChatRoomPolicy.defaults.features == .hostDefault)
    }

    @Test func thePolicyReadsTheFeaturesAndTheUserStateOfTheRoom() {
        let state = Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: true)

        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(features: readAloudFeatures, state: state))

        #expect(policy.features == readAloudFeatures)
        #expect(policy.userState == state)
    }

    @Test func aRoomWithFeaturesStaysInTheChatList() throws {
        let readAloud = Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            readAloud: true
        )

        let rows = try listRows(for: [room(id: "lobby", features: readAloud)])

        #expect(rows.count == 1)
    }
}

private extension ChatRoomAdditionsPolicyTests {
    var readAloudFeatures: Chat.RoomFeatures {
        Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, readAloud: true)
    }

    func declarations(
        features: Chat.RoomFeatures = .hostDefault,
        state: Chat.RoomUserState = .default
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: nil,
            configGeneration: 0,
            preview: .hostDefault,
            footerConfig: nil,
            features: features,
            userState: state
        )
    }

    func room(id: String, features: Chat.RoomFeatures) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension("game.dot", roomId: id),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: id, name: id, icon: nil),
            roomDeclarations: declarations(features: features)
        )
    }

    /// The identifiers of the rows that the chat list shows.
    func listRows(for chats: [Chat.LocalModel]) throws -> [String] {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: FixedChatMessageDecoderFactory(decoders: []),
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )
        let peerMetadata = Chat.PeerMetadata(name: "Game", icon: .bot, input: .empty, moreActions: [])

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: chats.map { ChatWithPeerMetadata(chat: $0, peerMetadata: peerMetadata.list) },
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        return viewModel.contactsById.map(\.id)
    }
}
