import SubstrateSdk

public extension MessageExchange {
    struct Response {
        let requestId: String
        let responseCode: ResponseCode
    }
}

extension MessageExchange.Response: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        requestId = try String(scaleDecoder: scaleDecoder)
        responseCode = try MessageExchange.ResponseCode(
            scaleCode: UInt8(scaleDecoder: scaleDecoder)
        )
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try requestId.encode(scaleEncoder: scaleEncoder)
        try responseCode.scaleCode.encode(scaleEncoder: scaleEncoder)
    }
}

private extension MessageExchange.ResponseCode {
    init(scaleCode: UInt8) {
        switch scaleCode {
        case 0:
            self = .success
        default:
            self = .failure(scaleCode)
        }
    }

    var scaleCode: UInt8 {
        switch self {
        case .success:
            0
        case let .failure(value):
            value
        }
    }
}

public extension MessageExchange {
    enum ResponseCode {
        case success
        case failure(UInt8)
    }
}

public extension MessageExchange.ResponseCode {
    var isSuccess: Bool {
        if case .success = self {
            true
        } else {
            false
        }
    }
}
