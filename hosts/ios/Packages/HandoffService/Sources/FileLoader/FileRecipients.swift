import Foundation
import NovaCrypto
import CryptoKit
import SubstrateSdk

// Recipient which can claim and decrypt uploaded file
public struct FileRecipients {
    public let pubKeys: Set<MultiSigner>
    public let encryptor: FileEncrypting

    public init(pubKeys: Set<MultiSigner>, encryptor: FileEncrypting) {
        self.pubKeys = pubKeys
        self.encryptor = encryptor
    }

    public init(ticket: FileTicket) throws {
        let pubKey = try ticket.deriveMultiSigner()
        pubKeys = [pubKey]

        encryptor = try ticket.deriveEncryptor()
    }
}
