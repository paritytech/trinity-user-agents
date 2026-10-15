import Foundation
import Testing
@testable import polkadot_app

/// The stored forms of the Chat Extension v2 additions: one version byte, then the SCALE bytes of the wire value in
/// wire field order (07-LATER §4.4). The bytes after the version byte are what the core encodes.
struct ChatRoomAdditionsCodingTests {
    @Test func theDefaultFeaturesAreTheVersionByteAndFiveEmptyFields() throws {
        #expect(try Chat.RoomFeatures.hostDefault.storedBytes() == Data([1, 0, 0, 0, 0, 0]))
    }

    @Test func everyFeatureHasItsWireLayout() throws {
        let features = Chat.RoomFeatures(
            preimageAttachments: true,
            search: true,
            calls: Chat.RoomCalls(audio: true, video: false),
            threadOf: Chat.ThreadParent(roomId: "r", messageId: "m"),
            readAloud: true
        )

        let bytes = try features.storedBytes()

        // version | preimage, search | Some(audio, video) | Some("r", "m") | read aloud
        #expect(bytes == Data([1, 1, 1, 1, 1, 0, 1, 4, 0x72, 4, 0x6D, 1]))
        #expect(Chat.RoomFeatures.decodedIfStored(bytes) == features)
    }

    @Test func metaHasItsWireLayout() throws {
        let meta = Chat.MessageMeta(
            thread: Chat.ThreadSummary(roomId: "t", replyCount: 3, lastReplyAt: 5, participantIds: ["a"]),
            branch: Chat.BranchPosition(index: 1, count: 2),
            reasoning: Chat.Reasoning(durationMs: nil)
        )

        let bytes = try meta.storedBytes()

        let thread: [UInt8] = [1, 4, 0x74, 3, 0, 0, 0, 1, 5, 0, 0, 0, 0, 0, 0, 0, 4, 4, 0x61]
        let branch: [UInt8] = [1, 1, 0, 2, 0]
        let reasoning: [UInt8] = [1, 0]
        #expect(bytes == Data([1] + thread + branch + reasoning))
        #expect(Chat.MessageMeta.decodedIfStored(bytes) == meta)
    }

    @Test func emptyMetaIsThreeEmptyOptions() throws {
        #expect(try Chat.MessageMeta.none.storedBytes() == Data([1, 0, 0, 0]))
        #expect(Chat.MessageMeta.none.isEmpty)
    }

    @Test func bytesOfAnUnknownVersionOrWithExtraBytesReadAsNothing() {
        #expect(Chat.RoomFeatures.decodedIfStored(Data([2, 0, 0, 0, 0, 0])) == nil)
        #expect(Chat.RoomFeatures.decodedIfStored(Data([1, 0, 0, 0, 0, 0, 9])) == nil)
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
