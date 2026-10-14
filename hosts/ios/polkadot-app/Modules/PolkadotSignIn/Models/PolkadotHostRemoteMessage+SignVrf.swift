import Foundation
import KeyDerivation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    struct SignVrfRequest {
        let callingProductId: ProductId
        let payload: SignVrfPayload
    }

    typealias SignVrfHostResult = HostResult<VrfSignatureWire, SignVrfWireError>

    struct VrfSignatureWire {
        static let preOutputLength = 32
        static let proofLength = 64

        let preOutput: Data
        let proof: Data
    }

    enum SignVrfWireError: Error {
        case notConnected
        case rejected
        case unknown(String)
    }

    enum SignVrfWireCodingError: Error {
        case invalidSignatureLength
    }
}

extension PolkadotHostRemoteMessage.VrfSignatureWire {
    init(signature: VrfSignature) {
        preOutput = signature.preOutput
        proof = signature.proof
    }
}

extension PolkadotHostRemoteMessage.SignVrfWireError {
    init(wrapping error: Error) {
        self =
            switch SignVrfError.wrapping(error) {
            case .rejected:
                .rejected
            case let .transcriptTooLarge(reason),
                 let .unknown(reason):
                .unknown(reason)
            }
    }
}

// MARK: - SCALE Coding

extension VrfTranscriptItem: @retroactive ScaleDecodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        try self.init(
            label: Data(scaleDecoder: scaleDecoder),
            value: Data(scaleDecoder: scaleDecoder)
        )
    }
}

extension VrfTranscriptItem: @retroactive ScaleEncodable {
    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try label.encode(scaleEncoder: scaleEncoder)
        try value.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.SignVrfRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        callingProductId = try String(scaleDecoder: scaleDecoder)
        payload = try SignVrfPayload(
            account: ProductAccountId(scaleDecoder: scaleDecoder),
            transcriptLabel: Data(scaleDecoder: scaleDecoder),
            items: [VrfTranscriptItem](scaleDecoder: scaleDecoder)
        )
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try callingProductId.encode(scaleEncoder: scaleEncoder)
        try payload.account.encode(scaleEncoder: scaleEncoder)
        try payload.transcriptLabel.encode(scaleEncoder: scaleEncoder)
        try payload.items.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.VrfSignatureWire: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        preOutput = try scaleDecoder.readAndConfirm(count: Self.preOutputLength)
        proof = try scaleDecoder.readAndConfirm(count: Self.proofLength)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        guard preOutput.count == Self.preOutputLength, proof.count == Self.proofLength else {
            throw PolkadotHostRemoteMessage.SignVrfWireCodingError.invalidSignatureLength
        }

        scaleEncoder.appendRaw(data: preOutput)
        scaleEncoder.appendRaw(data: proof)
    }
}

extension PolkadotHostRemoteMessage.SignVrfWireError: MessageExchange.CodableMessage {
    private var scaleIndex: UInt8 {
        switch self {
        case .notConnected: 0
        case .rejected: 1
        case .unknown: 2
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = .notConnected
        case 1:
            self = .rejected
        case 2:
            self = try .unknown(String(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        if case let .unknown(reason) = self {
            try reason.encode(scaleEncoder: scaleEncoder)
        }
    }
}
