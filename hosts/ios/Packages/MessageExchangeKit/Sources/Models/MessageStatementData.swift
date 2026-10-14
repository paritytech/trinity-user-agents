import Foundation
import SubstrateSdk

public enum StatementData<M: MessageExchange.CodableMessage> {
    case request(MessageExchange.Request<M>)
    case response(MessageExchange.Response)
    case multirequest(MultiDeviceRequest)
    case multiresponse(MultiDeviceResponse)
}

extension StatementData: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .request: 0
        case .response: 1
        case .multirequest: 2
        case .multiresponse: 3
        }
    }

    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            let value = try MessageExchange.Request<M>(scaleDecoder: scaleDecoder)
            self = .request(value)
        case 1:
            let value = try MessageExchange.Response(scaleDecoder: scaleDecoder)
            self = .response(value)
        case 2:
            let value = try MultiDeviceRequest(scaleDecoder: scaleDecoder)
            self = .multirequest(value)
        case 3:
            let value = try MultiDeviceResponse(scaleDecoder: scaleDecoder)
            self = .multiresponse(value)
        default:
            throw DynamicScaleDecoderError.unexpectedEnumCase
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .request(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .response(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .multirequest(value):
            try value.encode(scaleEncoder: scaleEncoder)
        case let .multiresponse(value):
            try value.encode(scaleEncoder: scaleEncoder)
        }
    }
}
