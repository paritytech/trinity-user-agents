import Foundation
import SubstrateSdk

public extension MessageExchange {
    struct Request<M: MessageExchange.CodableMessage> {
        public let requestId: String
        public let messages: [M]

        public init(requestId: String, messages: [M]) {
            self.requestId = requestId
            self.messages = messages
        }
    }
}

extension MessageExchange.Request: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        requestId = try String(scaleDecoder: scaleDecoder)
        messages = try [M](scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try requestId.encode(scaleEncoder: scaleEncoder)
        try messages.encode(scaleEncoder: scaleEncoder)
    }
}
