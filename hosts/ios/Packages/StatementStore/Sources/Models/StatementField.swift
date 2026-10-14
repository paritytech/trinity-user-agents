import Foundation
import SubstrateSdk
import BigInt

public enum StatementFieldConstants {
    public static let fixedFieldSize = 32
}

public enum StatementField {
    case proof(StatementProof)
    case expiry(UInt64)
    case channel(Data)
    case topic1(Data)
    case topic2(Data)
    case topic3(Data)
    case topic4(Data)
    case scaleEncodedPayload(Data)

    func getScaleEncodedPayload() -> Data? {
        guard case let .scaleEncodedPayload(data) = self else {
            return nil
        }
        return data
    }

    func getTopic1() -> Data? {
        guard case let .topic1(data) = self else {
            return nil
        }
        return data
    }

    func getTopic2() -> Data? {
        guard case let .topic2(data) = self else {
            return nil
        }
        return data
    }

    func getTopic3() -> Data? {
        guard case let .topic3(data) = self else {
            return nil
        }
        return data
    }

    func getTopic4() -> Data? {
        guard case let .topic4(data) = self else {
            return nil
        }
        return data
    }

    func getProof() -> StatementProof? {
        guard case let .proof(proof) = self else {
            return nil
        }
        return proof
    }

    func getExpiry() -> UInt64? {
        guard case let .expiry(priority) = self else {
            return nil
        }
        return priority
    }

    func getChannel() -> Data? {
        guard case let .channel(channel) = self else {
            return nil
        }
        return channel
    }
}

extension StatementField: ScaleCodable {
    var scaleIndex: UInt8 {
        switch self {
        case .proof: 0
        case .expiry: 2
        case .channel: 3
        case .topic1: 4
        case .topic2: 5
        case .topic3: 6
        case .topic4: 7
        case .scaleEncodedPayload: 8
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .proof(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .expiry(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .channel(value),
             let .topic1(value),
             let .topic2(value),
             let .topic3(value),
             let .topic4(value):
            scaleEncoder.appendRaw(data: value)
        case let .scaleEncodedPayload(data):
            scaleEncoder.appendRaw(data: data)
        }
    }

    public init(scaleDecoder: any ScaleDecoding) throws {
        let scaleIndex = try UInt8(scaleDecoder: scaleDecoder)

        switch scaleIndex {
        case 0:
            let value = try StatementProof(scaleDecoder: scaleDecoder)
            self = .proof(value)
        case 2:
            let value = try UInt64(scaleDecoder: scaleDecoder)
            self = .expiry(value)
        case 3:
            let value = try scaleDecoder.readAndConfirm(count: StatementFieldConstants.fixedFieldSize)
            self = .channel(value)
        case 4:
            let value = try scaleDecoder.readAndConfirm(count: StatementFieldConstants.fixedFieldSize)
            self = .topic1(value)
        case 5:
            let value = try scaleDecoder.readAndConfirm(count: StatementFieldConstants.fixedFieldSize)
            self = .topic2(value)
        case 6:
            let value = try scaleDecoder.readAndConfirm(count: StatementFieldConstants.fixedFieldSize)
            self = .topic3(value)
        case 7:
            let value = try scaleDecoder.readAndConfirm(count: StatementFieldConstants.fixedFieldSize)
            self = .topic4(value)
        case 8:
            let data = try scaleDecoder.readAndConfirm(count: scaleDecoder.remained)
            self = .scaleEncodedPayload(data)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }
}

public enum StatementProof: Equatable {
    case sr25519(signature: Data, signer: Data)
}

extension StatementProof: ScaleCodable {
    public static let signatureSize = 64
    public static let signerSize = 32

    var scaleIndex: UInt8 {
        switch self {
        case .sr25519: 0
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .sr25519(signature, signer):
            scaleEncoder.appendRaw(data: signature)
            scaleEncoder.appendRaw(data: signer)
        }
    }

    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            let signature = try scaleDecoder.readAndConfirm(count: Self.signatureSize)
            let signer = try scaleDecoder.readAndConfirm(count: Self.signerSize)

            self = .sr25519(signature: signature, signer: signer)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }
}
