import Foundation
import Testing
@testable import polkadot_app

/// The stored forms of the Chat Extension v2 additions and of the contact of a room: one version byte, then the SCALE
/// bytes of the value (07-LATER §4.4). The bytes of the features after the version byte are what the core encodes.
struct ChatRoomAdditionsCodingTests {
    @Test func theDefaultFeaturesAreTheVersionByteAndTwoEmptyFields() throws {
        #expect(try Chat.RoomFeatures.hostDefault.storedBytes() == Data([2, 0, 0]))
    }

    @Test func everyFeatureHasItsWireLayout() throws {
        let features = Chat.RoomFeatures(calls: Chat.RoomCalls(audio: true, video: false), hopAttachments: true)

        let bytes = try features.storedBytes()

        // version | Some(audio, video) | HOP
        #expect(bytes == Data([2, 1, 1, 0, 1]))
        #expect(Chat.RoomFeatures.decodedIfStored(bytes) == features)
    }

    @Test func bytesOfAnUnknownVersionOrWithExtraBytesReadAsNothing() {
        #expect(Chat.RoomFeatures.decodedIfStored(Data([1, 0, 0, 0, 0, 0, 0])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data([2, 0, 0, 9])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data([2, 0])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data()) == nil)
    }

    @Test func storedFeaturesThatDoNotDecodeReadAsTheHostDefault() {
        let declarations = Chat.RoomDeclarations(entity: nil)

        #expect(declarations.features == .hostDefault)
        #expect(declarations.contact == nil)
    }

    @Test(arguments: [
        Chat.RoomContact.State.invite, .outgoing, .incoming, .established, .blocked, .peerLeft
    ])
    func aContactKeepsItsAccountItsNameAndItsState(state: Chat.RoomContact.State) throws {
        let contact = Chat.RoomContact(account: Data(repeating: 5, count: 32), username: "alice", state: state)

        let bytes = try contact.storedBytes()

        #expect(bytes.count == 1 + 32 + 1 + 1 + 5 + 1)
        #expect(Chat.RoomContact.decodedIfStored(bytes) == contact)
    }

    @Test func aContactWithoutAUsernameKeepsNone() throws {
        let contact = Chat.RoomContact(account: Data(repeating: 5, count: 32), username: nil, state: .incoming)

        #expect(try Chat.RoomContact.decodedIfStored(contact.storedBytes()) == contact)
    }

    @Test func aContactNeedsAnAccountOf32Bytes() {
        let contact = Chat.RoomContact(account: Data(repeating: 5, count: 31), username: nil, state: .incoming)

        #expect(throws: (any Error).self) { try contact.storedBytes() }
    }

    @Test func theLastMessageOfAPreviewKeepsItsCallAndItsEdit() throws {
        let message = Chat.RoomPreviewMessage(
            messageId: "m1",
            status: .outgoing(.delivered),
            timestamp: 1_000,
            content: .call(.offer(.init(sdp: Data(), purpose: .video))),
            relatedMessages: [
                Chat.RelatedLocalMessage(
                    messageId: "m1/answer",
                    timestamp: 2_000,
                    content: .call(.answer(.init(offerId: "m1", sdp: Data()))),
                    status: .outgoing(.delivered)
                )
            ]
        )

        let decoded = try #require(Chat.RoomPreviewMessage.decodedIfStored(message.storedBytes()))

        #expect(decoded == message)
        #expect(decoded.localMessage(chatId: .chatExtension("chat.dot", roomId: "dm")).resolveCallState() == .active)
    }
}
