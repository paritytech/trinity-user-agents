import Foundation
import NovaCrypto
import SubstrateSdk

public protocol RecipientProofProviding {
    func getProof(for dataHash: FileHash, context: Data) async throws -> MultiSignature
}

public class SR25519RecipientProofProvider: RecipientProofProviding {
    let signer: SNSigner

    public init(ticket: FileTicket) throws {
        let keypair = try ticket.deriveSigningKeypair()
        signer = SNSigner(keypair: keypair)
    }

    public init(signer: SNSigner) {
        self.signer = signer
    }

    public func getProof(for dataHash: FileHash, context: Data) async throws -> MultiSignature {
        let payload = try (context + dataHash).blake2b32()
        let signature = try signer.sign(payload).rawData()
        return MultiSignature.sr25519(data: signature)
    }
}
