import Foundation
import SubstrateSdk

/// JSON-RPC request id preserving the wire type (jsonrpsee emits numbers;
/// string ids tolerated so restoration is faithful either way). Same wire
/// shape as the node-assigned subscription id, so the SDK enum is reused.
public typealias TrUAPIRpcId = JSONRPCSubscriptionId

/// One parsed outgoing core frame; `unsupported` marks anything the adapter
/// answers by logging instead of forwarding (batches, responses, non-JSON).
public enum TrUAPIRpcFrame {
    case request(id: TrUAPIRpcId, method: String, params: JSON?)
    case notification(method: String, params: JSON?)
    case unsupported

    /// Raw wire shape: `method` is required (responses and batches fail
    /// here), `params` stays opaque for verbatim forwarding.
    private struct RawFrame: Decodable {
        let id: TrUAPIRpcId?
        let method: String
        let params: JSON?
    }

    public static func parse(_ json: String) -> TrUAPIRpcFrame {
        guard
            let data = json.data(using: .utf8),
            let frame = try? JSONDecoder().decode(RawFrame.self, from: data)
        else {
            return .unsupported
        }

        if let id = frame.id {
            return .request(id: id, method: frame.method, params: frame.params)
        }
        return .notification(method: frame.method, params: frame.params)
    }
}
