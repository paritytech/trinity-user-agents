import Foundation
import MessageExchangeKit
import SubstrateSdk

enum IdentityProofFactory {
    private static let context = "mds-chat-request"

    static func makeProof(
        identityAccountId: Data,
        deviceAccountId: Data,
        sharedSecret: Data
    ) throws -> Chat.IdentityProof {
        let payload = try encodePayload(
            identityAccountId: identityAccountId,
            deviceAccountId: deviceAccountId
        )
        let proof = try payload.blake2b32WithKey(sharedSecret)

        return Chat.IdentityProof(
            identityAccountId: identityAccountId,
            proof: proof
        )
    }

    static func verifyProof(
        _ proof: Chat.IdentityProof,
        deviceAccountId: Data,
        sharedSecret: Data
    ) throws {
        let payload = try encodePayload(identityAccountId: proof.identityAccountId, deviceAccountId: deviceAccountId)
        let expectedProof = try payload.blake2b32WithKey(sharedSecret)

        guard expectedProof == proof.proof else {
            throw IdentityProofError.invalidProof
        }
    }
}

private extension IdentityProofFactory {
    static func encodePayload(
        identityAccountId: Data,
        deviceAccountId: Data
    ) throws -> Data {
        let encoder = ScaleEncoder()
        encoder.appendRaw(data: identityAccountId)
        encoder.appendRaw(data: deviceAccountId)
        try context.encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}

enum IdentityProofError: Error {
    case invalidProof
}
