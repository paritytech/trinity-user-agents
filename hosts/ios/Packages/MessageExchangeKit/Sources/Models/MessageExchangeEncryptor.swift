import Foundation
import CryptoKit

public protocol MessageExchangeEncrypting {
    var sharedSecret: Data { get }

    func encrypt(_ message: Data) throws -> Data
    func decrypt(_ message: Data) throws -> Data
}

public final class ChaChaPolyEncryptor {
    private let internalSharedSecret: SharedSecret
    private let internalSymmetricKey: SymmetricKey

    public init(sharedSecret: SharedSecret) {
        internalSharedSecret = sharedSecret
        internalSymmetricKey = sharedSecret.hkdfDerivedSymmetricKey(
            using: SHA256.self,
            salt: Data(),
            sharedInfo: Data(),
            outputByteCount: 32
        )
    }
}

extension ChaChaPolyEncryptor: MessageExchangeEncrypting {
    public var sharedSecret: Data {
        internalSharedSecret.withUnsafeBytes { Data($0) }
    }

    public func encrypt(_ message: Data) throws -> Data {
        try ChaChaPoly.seal(message, using: internalSymmetricKey).combined // nonce = 12, tag = 16
    }

    public func decrypt(_ message: Data) throws -> Data {
        let box = try ChaChaPoly.SealedBox(combined: message) // nonce = 12, tag = 16
        return try ChaChaPoly.open(box, using: internalSymmetricKey)
    }
}
