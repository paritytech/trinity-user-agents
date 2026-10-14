import Foundation
import SubstrateSdk
import MessageExchangeKit
import StatementStore

extension ChatRequest {
    struct RemoteModel: Equatable {
        let message: Chat.RequestMessage
        let proof: StatementProof
    }

    struct ValidatedRemoteModel: Equatable {
        let message: Chat.RequestMessage
        let peerAccountId: AccountId
        let peerStatementAccountId: AccountId

        var requestId: String {
            message.messageId
        }

        var peerDevice: Chat.PeerDevice? {
            message.content.extractSenderDevice(statementAccountId: peerStatementAccountId)
        }
    }

    struct EncryptedRemoteModel: Equatable {
        let encryptionPubKey: Data
        let encryptedData: Data
    }

    struct ProofPayload: Equatable {
        let message: Chat.RequestMessage
        let requestAcceptorId: AccountId
    }
}

extension ChatRequest.RemoteModel: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        message = try Chat.RequestMessage(scaleDecoder: scaleDecoder)
        proof = try StatementProof(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try message.encode(scaleEncoder: scaleEncoder)
        try proof.encode(scaleEncoder: scaleEncoder)
    }
}

extension ChatRequest.EncryptedRemoteModel: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        encryptionPubKey = try Data(scaleDecoder: scaleDecoder)
        encryptedData = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try encryptionPubKey.encode(scaleEncoder: scaleEncoder)
        try encryptedData.encode(scaleEncoder: scaleEncoder)
    }
}

extension ChatRequest.ProofPayload: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        message = try Chat.RequestMessage(scaleDecoder: scaleDecoder)
        requestAcceptorId = try AccountId(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try message.encode(scaleEncoder: scaleEncoder)
        try requestAcceptorId.encode(scaleEncoder: scaleEncoder)
    }
}
