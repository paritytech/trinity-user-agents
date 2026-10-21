import Foundation
import Testing
@testable import polkadot_app

/// The stored forms of the Chat Extension v2 additions: one version byte, then the SCALE bytes of the wire value in
/// wire field order (07-LATER §4.4). The bytes after the version byte are what the core encodes.
struct ChatRoomAdditionsCodingTests {
    @Test func theDefaultFeaturesAreTheVersionByteAndSixEmptyFields() throws {
        #expect(try Chat.RoomFeatures.hostDefault.storedBytes() == Data([1, 0, 0, 0, 0, 0, 0]))
    }

    @Test func everyFeatureHasItsWireLayout() throws {
        let features = Chat.RoomFeatures(
            preimageAttachments: true,
            search: true,
            calls: Chat.RoomCalls(audio: true, video: false),
            readAloud: true,
            sealedAttachments: true,
            hopAttachments: true
        )

        let bytes = try features.storedBytes()

        // version | preimage, search | Some(audio, video) | read aloud | sealed, HOP
        #expect(bytes == Data([1, 1, 1, 1, 1, 0, 1, 1, 1]))
        #expect(Chat.RoomFeatures.decodedIfStored(bytes) == features)
    }

    @Test func bytesOfAnUnknownVersionOrWithExtraBytesReadAsNothing() {
        #expect(Chat.RoomFeatures.decodedIfStored(Data([2, 0, 0, 0, 0, 0])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data([1, 0, 0, 0, 0, 0, 0, 9])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data([1, 0, 0])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data()) == nil)
    }

    @Test func storedFeaturesThatDoNotDecodeReadAsTheHostDefault() {
        let declarations = Chat.RoomDeclarations(entity: nil)

        #expect(declarations.features == .hostDefault)
        #expect(declarations.userState == .default)
    }

    @Test(arguments: [
        (UInt64?.none, Int64(0)),
        (UInt64?.some(UInt64.max), Int64.max),
        (UInt64?.some(1_700_000_000_000), Int64(1_700_000_000_000))
    ])
    func theMuteEndHasAStoredForm(_ mutedUntil: UInt64?, _ stored: Int64) {
        let state = Chat.RoomUserState(mutedUntil: mutedUntil, archived: false)

        #expect(state.storedMutedUntil == stored)
        #expect(Chat.RoomUserState.mutedUntil(stored: stored) == mutedUntil)
    }

    @Test func aMuteIsActiveUntilItsEnd() {
        let now = Date(timeIntervalSince1970: 1_000)

        #expect(!Chat.RoomUserState.default.isMuted(at: now))
        #expect(Chat.RoomUserState(mutedUntil: .max, archived: false).isMuted(at: now))
        #expect(Chat.RoomUserState(mutedUntil: 1_000_001, archived: false).isMuted(at: now))
        #expect(!Chat.RoomUserState(mutedUntil: 1_000_000, archived: false).isMuted(at: now))
    }
}
