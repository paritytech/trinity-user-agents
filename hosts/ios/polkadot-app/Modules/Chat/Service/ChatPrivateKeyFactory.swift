import Foundation
import CryptoKit
import KeyDerivation

protocol ChatPrivateKeyMaking {
    func derivePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey
}

enum ChatPrivateKeyFactoryError: Error {
    case unknownEncryptionKeyId(String)
}

/// Key-agreement derivation domains for built-in chat surfaces.
/// Raw values double as the persisted `encryptionKeyId` identifiers.
enum ChatEncryptionDomain: String {
    /// Post-MDS this key is shared across devices and authenticates chat requests;
    /// per-device encryption keys are generated randomly.
    case mainChat = "chat"
    /// E2E encryption in the SSO transport.
    case sso
}

/// Derives the X25519 key for a domain from the keyed-hash chain
/// rooted at `hash(root_entropy, "ecdh")`; the 32-byte material is used directly
/// as the private key (clamped per RFC 7748).
final class ChatPrivateKeyFactory {
    private let domain: ChatEncryptionDomain
    private let keyMaterialDeriver: EcdhKeyMaterialDeriving

    init(domain: ChatEncryptionDomain, keyMaterialDeriver: EcdhKeyMaterialDeriving) {
        self.domain = domain
        self.keyMaterialDeriver = keyMaterialDeriver
    }

    convenience init(encryptionKeyId: String, entropyManager: RootEntropyManaging) throws {
        guard let domain = ChatEncryptionDomain(rawValue: encryptionKeyId) else {
            throw ChatPrivateKeyFactoryError.unknownEncryptionKeyId(encryptionKeyId)
        }

        self.init(
            domain: domain,
            keyMaterialDeriver: EcdhKeyMaterialDeriver(entropyManager: entropyManager)
        )
    }
}

extension ChatPrivateKeyFactory: ChatPrivateKeyMaking {
    func derivePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey {
        let material = try keyMaterialDeriver.deriveKeyMaterial(for: domain.rawValue)
        return try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: material)
    }
}
