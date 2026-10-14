import Foundation
import CryptoKit

public protocol FileEncrypting {
    func encrypt(_ data: Data) throws -> Data
    func decrypt(_ encryptedData: Data) throws -> Data
}

public final class ChaChaPolyFileEncryptor {
    let symmetricKey: SymmetricKey

    public init(rawKey: Data) {
        symmetricKey = SymmetricKey(data: rawKey)
    }

    public init(symmetricKey: SymmetricKey) {
        self.symmetricKey = symmetricKey
    }
}

extension ChaChaPolyFileEncryptor: FileEncrypting {
    public func encrypt(_ data: Data) throws -> Data {
        try ChaChaPoly.seal(data, using: symmetricKey).combined // nonce = 12, tag = 16
    }

    public func decrypt(_ encryptedData: Data) throws -> Data {
        let box = try ChaChaPoly.SealedBox(combined: encryptedData) // nonce = 12, tag = 16
        return try ChaChaPoly.open(box, using: symmetricKey)
    }
}
