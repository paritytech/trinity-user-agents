import ChainRegistry
import Foundation
@testable import PolkadotUI
import Testing
@testable import polkadot_app

/// The room policy carries the features, the user state and the message meta of the Chat Extension v2 additions, so
/// each later item reads one value (07-LATER §4.4).
struct ChatRoomAdditionsPolicyTests {
    @Test func aRoomThatDeclaresNothingHasTheDefaults() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations())

        #expect(policy.features == .hostDefault)
        #expect(policy.userState == .default)
        #expect(policy.messageMeta.isEmpty)
        #expect(!policy.isThreadRoom)
        #expect(ChatRoomPolicy.defaults.features == .hostDefault)
    }

    @Test func thePolicyReadsTheFeaturesAndTheUserStateOfTheRoom() {
        let state = Chat.RoomUserState(mutedUntil: Chat.RoomUserState.mutedForever, archived: true)

        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(features: threadFeatures, state: state))

        #expect(policy.features == threadFeatures)
        #expect(policy.userState == state)
        #expect(policy.isThreadRoom)
    }

    @Test func theMetaOfAMessageComesFromTheIndexOfTheRoom() {
        let reasoning = Chat.MessageMeta(thread: nil, branch: nil, reasoning: Chat.Reasoning(durationMs: 4_000))
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations())
            .with(messageMeta: Chat.MessageMetaIndex(["m1": reasoning]))

        #expect(policy.meta(for: "m1") == reasoning)
        #expect(policy.meta(for: "m2") == nil)
    }

    @Test func aThreadRoomIsNotInTheChatList() throws {
        let rows = try listRows(for: [
            room(id: "lobby", features: .hostDefault),
            room(id: "thread-1", features: threadFeatures)
        ])

        #expect(rows == [Chat.Id.chatExtension("game.dot", roomId: "lobby").rawRepresentation])
    }

    @Test func aRoomWithOtherFeaturesStaysInTheChatList() throws {
        let readAloud = Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: nil,
            readAloud: true
        )

        let rows = try listRows(for: [room(id: "lobby", features: readAloud)])

        #expect(rows.count == 1)
    }
}

private extension ChatRoomAdditionsPolicyTests {
    var threadFeatures: Chat.RoomFeatures {
        Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: Chat.ThreadParent(roomId: "lobby", messageId: "m1"),
            readAloud: false
        )
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
            toolbar: nil,
            suggestions: nil,
            commands: nil,
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
                establishedChats: chats.map { ChatWithPeerMetadata(chat: $0, peerMetadata: peerMetadata) },
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        return viewModel.contactsById.map(\.id)
    }
}
