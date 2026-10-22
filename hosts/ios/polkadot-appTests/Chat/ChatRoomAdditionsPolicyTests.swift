import Foundation
import Testing
@testable import polkadot_app

/// The room policy carries the features and the contact of a room, so each later item reads one value
/// (07-LATER §4.4).
struct ChatRoomAdditionsPolicyTests {
    @Test func aRoomThatDeclaresNothingHasTheDefaults() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations())

        #expect(policy.features == .hostDefault)
        #expect(policy.contact == nil)
        #expect(ChatRoomPolicy.defaults.features == .hostDefault)
    }

    @Test func thePolicyReadsTheFeaturesAndTheContactOfTheRoom() {
        let features = Chat.RoomFeatures(calls: Chat.RoomCalls(audio: true, video: true), hopAttachments: true)
        let contact = ProductRoomTestData.contact(.established)

        let policy = ChatRoomPolicy(footer: nil, declarations: declarations(features: features, contact: contact))

        #expect(policy.features == features)
        #expect(policy.contact == contact)
        #expect(policy.toolbar == [.audioCall, .videoCall])
        #expect(policy.sendsMedia)
    }

    @Test func aRoomWithoutHopSendsNoMedia() {
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations())

        #expect(!policy.sendsMedia)
    }
}

private extension ChatRoomAdditionsPolicyTests {
    func declarations(
        features: Chat.RoomFeatures = .hostDefault,
        contact: Chat.RoomContact? = nil
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            config: nil,
            configGeneration: 0,
            preview: .hostDefault,
            features: features,
            contact: contact
        )
    }
}
