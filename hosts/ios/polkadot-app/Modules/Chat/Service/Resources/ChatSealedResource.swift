import CryptoKit
import Foundation
import SubstrateSdk

/// A file that a host sealed for a product room (`sealed_attachments`): XChaCha20-Poly1305 of the file under `key`
/// and `nonce`, with the associated data `truapi/chat/sealed/v1`, stored on the Bulletin chain in `parts`.
///
/// The product sends the resource through its own encrypted channel. The chain keeps only the ciphertext.
struct ChatSealedResource: Hashable, Sendable, Codable {
    let parts: [Data]
    let key: Data
    let nonce: Data
    /// The length of the ciphertext.
    let size: UInt64
    /// SHA-512 of the ciphertext.
    let digest: Data

    static let associatedData = Data("truapi/chat/sealed/v1".utf8)

    /// Reads the parts in order through `lookup`, checks each part against its key, and opens the ciphertext.
    func read(lookup: (Data) async -> Data?) async -> Data? {
        var ciphertext = Data()
        for part in parts {
            guard let value = await lookup(part), (try? value.blake2b32()) == part else {
                return nil
            }
            ciphertext.append(value)
            guard ciphertext.count <= size else { return nil }
        }
        return open(ciphertext)
    }

    /// The file in `ciphertext`, or nil when the ciphertext does not match the size, the digest or the key.
    func open(_ ciphertext: Data) -> Data? {
        guard UInt64(ciphertext.count) == size, Data(SHA512.hash(data: ciphertext)) == digest else {
            return nil
        }

        return XChaCha20Poly1305.open(ciphertext, key: key, nonce: nonce, authenticating: Self.associatedData)
    }
}
