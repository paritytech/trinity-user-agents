import Foundation
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    enum SigningRequest: Equatable {
        case transaction(SignTransactionPayload<ProductAccountId>)
        case rawPayload(SigningRawPayload)

        var account: ProductAccountId {
            switch self {
            case let .transaction(transaction):
                transaction.account
            case let .rawPayload(signingRawPayload):
                signingRawPayload.account
            }
        }
    }
}

extension PolkadotHostRemoteMessage.SigningRequest: ScaleCodable {
    var scaleIndex: UInt8 {
        switch self {
        case .transaction: 0
        case .rawPayload: 1
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = try .transaction(.init(scaleDecoder: scaleDecoder))
        case 1:
            self = try .rawPayload(.init(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .transaction(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .rawPayload(value):
            try value.encode(scaleEncoder: scaleEncoder)
        }
    }
}
