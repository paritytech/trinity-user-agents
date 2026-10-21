import CryptoKit
import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

/// The app opens a sealed file of a product room itself: it reads the parts, checks each against its key, and
/// decrypts the ciphertext that the core made. The vector comes from the `chacha20poly1305` crate of the core.
struct ChatSealedResourceTests {
    private static let key = Data(0 ..< 32)
    private static let nonce = Data(100 ..< 124)
    private static let file = Data("a sealed attachment of the chat".utf8)
    private static let ciphertext = Data(hex: """
    1d538ad59e42ece6a751e94bc70cd5a43ed526518b4aeac4e43044c4433f0cfa4dc9a5e93f656e666e4a97c2950479
    """)

    @Test func hChaCha20GivesTheSubkeyOfTheDraftVector() {
        let key = Data(0 ..< 32)
        let nonce = Data(hex: "000000090000004a0000000031415927")

        #expect(HChaCha20.subkey(key: key, nonce: nonce) == Data(hex: """
        82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc
        """))
    }

    @Test func aSealedFileOpensFromItsPartsInOrder() async throws {
        let parts = [Self.ciphertext.prefix(20), Self.ciphertext.dropFirst(20)].map { Data($0) }
        let stored = try Dictionary(uniqueKeysWithValues: parts.map { try ($0.blake2b32(), $0) })
        let resource = try ChatSealedResource(
            parts: parts.map { try $0.blake2b32() },
            key: Self.key,
            nonce: Self.nonce,
            size: UInt64(Self.ciphertext.count),
            digest: Data(SHA512.hash(data: Self.ciphertext))
        )

        #expect(await resource.read { stored[$0] } == Self.file)
    }

    @Test func aChangedPartOrAnotherKeyOpensNothing() async throws {
        let part = Self.ciphertext
        let resource = try ChatSealedResource(
            parts: [part.blake2b32()],
            key: Self.key,
            nonce: Self.nonce,
            size: UInt64(part.count),
            digest: Data(SHA512.hash(data: part))
        )
        var forged = part
        forged[0] ^= 1
        let otherKey = ChatSealedResource(
            parts: resource.parts,
            key: Data(repeating: 7, count: 32),
            nonce: Self.nonce,
            size: resource.size,
            digest: resource.digest
        )

        #expect(await resource.read { _ in forged } == nil)
        #expect(otherKey.open(part) == nil)
    }
}

private extension Data {
    init(hex: String) {
        let digits = Array(hex.filter(\.isHexDigit).utf8)
        self.init(stride(from: 0, to: digits.count, by: 2).map {
            UInt8(String(decoding: digits[$0 ... $0 + 1], as: UTF8.self), radix: 16) ?? 0
        })
    }
}
