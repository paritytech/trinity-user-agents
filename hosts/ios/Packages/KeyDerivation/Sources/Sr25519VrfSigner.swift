import Foundation
import NovaCrypto
import SubstrateSdk

/// One `append_message(label, value)` call against the merlin transcript being signed.
/// Bytes ride as hex in JSON contexts (the container bridge wire).
public struct VrfTranscriptItem: Equatable, Codable {
    @HexCodable public var label: Data
    @HexCodable public var value: Data

    public init(label: Data, value: Data) {
        _label = HexCodable(wrappedValue: label)
        _value = HexCodable(wrappedValue: value)
    }
}

/// An sr25519 (schnorrkel) VRF signature: the 32-byte pre-output and the 64-byte DLEQ proof.
public struct VrfSignature: Equatable {
    public let preOutput: Data
    public let proof: Data

    public init(preOutput: Data, proof: Data) {
        self.preOutput = preOutput
        self.proof = proof
    }
}

/// Pure transcript replayer over NovaCrypto's sr25519 VRF: `Transcript::new(transcriptLabel)`
/// followed by one `append_message(item.label, item.value)` per item, in order (RFC-0023).
/// Callers own the transcript shape their consuming runtime expects, including any bounds.
public enum Sr25519VrfSigner {
    public static func sign(
        wallet: RawKeypairProviding,
        transcriptLabel: Data,
        items: [VrfTranscriptItem]
    ) throws -> VrfSignature {
        let snPrivateKey = try SNPrivateKey(rawData: wallet.fetchRawSecretKey())
        let snPublicKey = try SNPublicKey(rawData: wallet.getRawPublicKey())
        let snKeypair = SNKeypair(privateKey: snPrivateKey, publicKey: snPublicKey)

        let signer = SNVrfSigner(keypair: snKeypair)

        let fields = items.map { SNVrfField(key: $0.label, value: $0.value) }

        let signature = try signer.sign(withLabel: transcriptLabel, fields: fields)

        return VrfSignature(
            preOutput: signature.preOutput,
            proof: signature.proof
        )
    }
}
