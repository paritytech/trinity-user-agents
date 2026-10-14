import Foundation
import SubstrateSdk

/// Node-assigned subscription id — the SDK's wire-type-preserving enum,
/// used verbatim so numeric and string ids round-trip to the core exactly
/// as the node assigned them.
public typealias TrUAPISubscriptionId = JSONRPCSubscriptionId

/// Typed envelopes the adapter synthesizes toward the core. The SDK's own
/// envelope models are engine-internal (UInt16 wire ids); these carry the
/// core's original ids.
struct TrUAPIRpcResponse<Result: Encodable>: Encodable {
    let jsonrpc: String = "2.0"
    let id: TrUAPIRpcId
    let result: Result
}

struct TrUAPIRpcErrorResponse: Encodable {
    let jsonrpc: String = "2.0"
    let id: TrUAPIRpcId
    let error: JSONRPCError
}

struct TrUAPIRpcNotification: Encodable {
    struct Params: Encodable {
        let subscription: TrUAPISubscriptionId
        let result: JSON
    }

    let jsonrpc: String = "2.0"
    let method: String
    let params: Params
}

/// One decoded incoming update frame: method, subscription id, payload.
struct TrUAPIRpcUpdateFrame: Decodable {
    struct Params: Decodable {
        let subscription: TrUAPISubscriptionId
        let result: JSON?
    }

    let method: String
    let params: Params
}

extension TrUAPISubscriptionId {
    /// Bridge from a JSON param value (string or number); nil for other shapes.
    init?(paramValue: JSON) {
        if let string = paramValue.stringValue {
            self = .string(string)
        } else if let number = paramValue.unsignedIntValue {
            self = .number(number)
        } else {
            return nil
        }
    }
}
