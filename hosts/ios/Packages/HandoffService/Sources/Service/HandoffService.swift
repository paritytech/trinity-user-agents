import Foundation
import SubstrateSdk
import NovaCrypto
import StructuredConcurrency

public protocol HandoffServicing {
    @discardableResult
    func submitData(
        _ data: Data,
        from sender: SenderProofProviding,
        recipients: Set<MultiSigner>
    ) async throws -> SubmittedData

    func claimData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws -> Data?

    func acknowledgeReceivedData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws
}

public final class HandoffService {
    let connection: JSONRPCEngine

    public init(connection: JSONRPCEngine) {
        self.connection = connection
    }
}

extension HandoffService: HandoffServicing {
    @discardableResult
    public func submitData(
        _ data: Data,
        from sender: SenderProofProviding,
        recipients: Set<MultiSigner>
    ) async throws -> SubmittedData {
        let dataHash = try data.blake2b32()

        let proof = try await sender.getProof(for: dataHash)

        let hexRecipients = try recipients.map { recipient in
            let data = try recipient.scaleEncoded()
            return HexCodable(wrappedValue: data)
        }

        let submission = try SubmissionModel(
            data: data,
            recipients: hexRecipients,
            signature: proof.signature.scaleEncoded(),
            signer: proof.sender.scaleEncoded(),
            submitTimestamp: proof.submitTimestamp
        )

        let result: SubmitResult = try await connection.asyncCallMethod(
            RPCMethod.submit,
            params: submission,
            options: JSONRPCOptions(resendOnReconnect: false)
        )

        return SubmittedData(hash: dataHash, poolStatus: result.poolStatus)
    }

    public func claimData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws -> Data? {
        let multiSignature = try await recipient.getProof(for: dataHash, context: RpcContext.claim)
        let signature = try multiSignature.scaleEncoded()

        let model = ClaimModel(hash: dataHash, signature: signature)

        let dataHex: String = try await connection.asyncCallMethod(
            RPCMethod.claim,
            params: model,
            options: JSONRPCOptions()
        )

        return try Data(hexString: dataHex)
    }

    public func acknowledgeReceivedData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws {
        let multiSignature = try await recipient.getProof(for: dataHash, context: RpcContext.ack)
        let signature = try multiSignature.scaleEncoded()

        let model = AckModel(hash: dataHash, signature: signature)

        do {
            try await connection.asyncCallVoidMethod(
                RPCMethod.ack,
                params: model,
                options: JSONRPCOptions()
            )
        } catch let error as JSONRPCError where error.code == HOPErrorCode.notFound {
            // Base spec: NotFound on ack is benign — the entry is already removed
            // (acked by another device or expired and promoted on-chain).
        }
    }
}
