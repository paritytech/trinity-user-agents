import Foundation
import CryptoKit

public protocol MessageExchangeEncryptionManaging {
    func makeEncryptorFactory(ownEncryptionKeyId: String) throws -> MessageExchangeEncryptionMaking
}

public class ClosureEncryptionManager: MessageExchangeEncryptionManaging {
    private let closure: (String) throws -> MessageExchangeEncryptionMaking

    public init(closure: @escaping (String) throws -> MessageExchangeEncryptionMaking) {
        self.closure = closure
    }

    public func makeEncryptorFactory(ownEncryptionKeyId: String) throws -> MessageExchangeEncryptionMaking {
        try closure(ownEncryptionKeyId)
    }
}

public protocol MessageExchangeEncryptionMaking {
    var localPublicKey: Data { get }
    var localPrivateKey: Data { get }

    func makeEncryptor(remotePublicKey: Data) throws -> MessageExchangeEncrypting
}

public final class X25519ChaChaPolyEncryptorFactory: MessageExchangeEncryptionMaking {
    private let privateKey: Curve25519.KeyAgreement.PrivateKey

    public init(privateKey: Curve25519.KeyAgreement.PrivateKey) {
        self.privateKey = privateKey
    }

    public var localPublicKey: Data {
        privateKey.publicKey.rawRepresentation
    }

    public var localPrivateKey: Data {
        privateKey.rawRepresentation
    }

    public func makeEncryptor(remotePublicKey: Data) throws -> MessageExchangeEncrypting {
        let publicKey = try Curve25519.KeyAgreement.PublicKey(rawRepresentation: remotePublicKey)
        let sharedKey = try privateKey.sharedSecretFromKeyAgreement(with: publicKey)
        return ChaChaPolyEncryptor(sharedSecret: sharedKey)
    }
}
