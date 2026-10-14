import Foundation

// Claimer that can sign proof and decrypt downloaded file
public struct FileClaimer {
    public let proofProvider: RecipientProofProviding
    public let decryptor: FileEncrypting

    public init(proofProvider: RecipientProofProviding, decryptor: FileEncrypting) {
        self.proofProvider = proofProvider
        self.decryptor = decryptor
    }

    public init(ticket: FileTicket) throws {
        proofProvider = try SR25519RecipientProofProvider(ticket: ticket)
        decryptor = try ticket.deriveEncryptor()
    }
}
