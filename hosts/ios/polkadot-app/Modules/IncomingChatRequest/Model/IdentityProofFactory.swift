import Foundation
import MessageExchangeKit
import SubstrateSdk

enum IdentityProofFactory {
    private static let context = "mds-chat-request"
    private static let cancelContext = "mds-chat-request-cancel"

    static func makeProof(
        identityAccountId: Data,
        deviceAccountId: Data,
        sharedSecret: Data
    ) throws -> Chat.IdentityProof {
        let payload = try encodePayload(
            identityAccountId: identityAccountId,
            deviceAccountId: deviceAccountId
        )
        return try keyedProof(identityAccountId: identityAccountId, payload: payload, sharedSecret: sharedSecret)
    }

    static func verifyProof(
        _ proof: Chat.IdentityProof,
        deviceAccountId: Data,
        sharedSecret: Data
    ) throws {
        let payload = try encodePayload(identityAccountId: proof.identityAccountId, deviceAccountId: deviceAccountId)
        try verifyKeyedProof(proof, payload: payload, sharedSecret: sharedSecret)
    }

    static func makeCancelProof(
        identityAccountId: Data,
        requestId: String,
        sharedSecret: Data
    ) throws -> Chat.IdentityProof {
        let payload = try encodeCancelPayload(identityAccountId: identityAccountId, requestId: requestId)
        return try keyedProof(identityAccountId: identityAccountId, payload: payload, sharedSecret: sharedSecret)
    }

    static func verifyCancelProof(
        _ proof: Chat.IdentityProof,
        requestId: String,
        sharedSecret: Data
    ) throws {
        let payload = try encodeCancelPayload(identityAccountId: proof.identityAccountId, requestId: requestId)
        try verifyKeyedProof(proof, payload: payload, sharedSecret: sharedSecret)
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

    static func encodeCancelPayload(identityAccountId: Data, requestId: String) throws -> Data {
        let encoder = ScaleEncoder()
        encoder.appendRaw(data: identityAccountId)
        try requestId.encode(scaleEncoder: encoder)
        try cancelContext.encode(scaleEncoder: encoder)
        return encoder.encode()
    }

    static func keyedProof(identityAccountId: Data, payload: Data, sharedSecret: Data) throws -> Chat.IdentityProof {
        try Chat.IdentityProof(identityAccountId: identityAccountId, proof: payload.blake2b32WithKey(sharedSecret))
    }

    static func verifyKeyedProof(_ proof: Chat.IdentityProof, payload: Data, sharedSecret: Data) throws {
        guard try payload.blake2b32WithKey(sharedSecret) == proof.proof else {
            throw IdentityProofError.invalidProof
        }
    }
}

enum IdentityProofError: Error {
    case invalidProof
}
