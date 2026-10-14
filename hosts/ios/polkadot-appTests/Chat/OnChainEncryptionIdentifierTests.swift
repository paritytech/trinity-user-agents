import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Testing

@testable import polkadot_app

/// Wire vectors for the on-chain 65-byte encryption-key container
/// (chat-spec RFC-0004 §4): `0x00 ‖ 32-byte X25519 key ‖ 32 zero bytes`.
/// The hex fixtures are normative and shared with the Android client.
@Suite("On-chain encryption identifier container")
struct OnChainEncryptionIdentifierTests {
    private static let keyHex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
    private static let containerHex = "00"
        + "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
        + "0000000000000000000000000000000000000000000000000000000000000000"

    private func decode(_ container: Data) throws -> Chat.OnChainEncryptionIdentifier {
        try Chat.OnChainEncryptionIdentifier.fromScaleEncoded(container)
    }

    @Test("Encodes to the pinned container bytes")
    func encodesToPinnedBytes() throws {
        let publicKey = try Chat.PublicKey(rawData: Data(hexString: Self.keyHex))
        let container = try Chat.OnChainEncryptionIdentifier.x25519(publicKey).scaleEncoded()

        #expect(container.count == Chat.OnChainEncryptionIdentifier.containerSize)
        #expect(container.toHex() == Self.containerHex)
    }

    @Test("Decodes the pinned container bytes back to the local public key")
    func decodesPinnedBytes() throws {
        let decoded = try decode(Data(hexString: Self.containerHex))

        #expect(decoded.localPublicKey.rawData.toHex() == Self.keyHex)
    }

    @Test("Ignores non-zero padding bytes on read")
    func ignoresPadding() throws {
        let container = try Data(hexString: "00" + Self.keyHex) + Data(repeating: 0xFF, count: 32)
        let decoded = try decode(container)

        #expect(decoded.localPublicKey.rawData.toHex() == Self.keyHex)
    }

    @Test("Rejects an unsupported keypair-type byte (legacy P-256 records start with 0x04)")
    func rejectsUnsupportedType() throws {
        let legacyContainer = try Data([0x04]) + Data(hexString: Self.keyHex) + Data(count: 32)

        #expect(throws: ScaleCodingError.self) {
            _ = try decode(legacyContainer)
        }
    }

    @Test("Rejects truncated containers", arguments: [32, 33, 64])
    func rejectsTruncated(size: Int) {
        #expect(throws: Error.self) {
            _ = try decode(Data(count: size))
        }
    }
}
