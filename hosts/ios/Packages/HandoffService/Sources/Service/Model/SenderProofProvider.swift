import Foundation
import SubstrateSdk
import FoundationExt

public struct SenderProof {
    public let sender: MultiSigner
    public let signature: MultiSignature
    public let submitTimestamp: UInt64

    public init(sender: MultiSigner, signature: MultiSignature, submitTimestamp: UInt64) {
        self.sender = sender
        self.signature = signature
        self.submitTimestamp = submitTimestamp
    }
}

public protocol SenderProofProviding {
    func getProof(for dataHash: FileHash) async throws -> SenderProof
}

public class SenderProofProvider: SenderProofProviding {
    public typealias SignatureFactory = (Data) async throws -> MultiSignature

    let sender: MultiSigner
    let signatureFactory: SignatureFactory

    public init(sender: MultiSigner, signatureFactory: @escaping SignatureFactory) {
        self.sender = sender
        self.signatureFactory = signatureFactory
    }

    public func getProof(for dataHash: FileHash) async throws -> SenderProof {
        // Matches onchain check: `blake2_256(HOP_SUBMIT_CONTEXT || blake2_256(data) || submit_timestamp.to_le_bytes())`
        let timestamp = UInt64(Date().timeIntervalSince1970.milliseconds)
        let payloadBuffer = RpcContext.submit + dataHash + Data(timestamp.littleEndianBytes)
        let payload = try payloadBuffer.blake2b32()

        let signature = try await signatureFactory(payload)

        return SenderProof(sender: sender, signature: signature, submitTimestamp: timestamp)
    }
}
