import Foundation
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    struct SigningRawPayload: Equatable {
        let account: ProductAccountId
        let type: PayloadType
    }
}

extension PolkadotHostRemoteMessage.SigningRawPayload: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        account = try ProductAccountId(scaleDecoder: scaleDecoder)
        type = try PayloadType(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try account.encode(scaleEncoder: scaleEncoder)
        try type.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.SigningRawPayload {
    enum PayloadType: Equatable {
        case bytes(Data)
        case payload(String)
    }
}

extension PolkadotHostRemoteMessage.SigningRawPayload.PayloadType: ScaleCodable {
    var scaleIndex: UInt8 {
        switch self {
        case .bytes: 0
        case .payload: 1
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            let value = try Data(scaleDecoder: scaleDecoder)
            self = .bytes(value)
        case 1:
            let value = try String(scaleDecoder: scaleDecoder)
            self = .payload(value)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .bytes(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .payload(value):
            try value.encode(scaleEncoder: scaleEncoder)
        }
    }
}
