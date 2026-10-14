import Foundation
import SubstrateSdk
import ChainRegistry

/// Recording JSONRPCEngine: captures calls and subscriptions, lets tests
/// fire completions/updates/failures. Non-final so MockChainConnection can
/// add the ChainConnection conformances on top.
// swiftlint:disable:next final_class
class MockJSONRPCEngine: JSONRPCEngine {
    struct RecordedCall {
        let method: String
        let params: JSON?
        let completion: (Result<JSON, Error>) -> Void
    }

    struct RecordedSubscription {
        let method: String
        let params: JSON?
        let unsubscribeMethod: String
        let resendOnReconnect: Bool
        let onSubscribed: ((JSONRPCSubscriptionId) -> Void)?
        let update: (JSON) -> Void
        let failure: (Error, Bool) -> Void
    }

    private(set) var calls: [UInt16: RecordedCall] = [:]
    private(set) var subscriptions: [UInt16: RecordedSubscription] = [:]
    private(set) var cancelled: [UInt16] = []
    private(set) var silentlyCancelled: [UInt16] = []
    private var nextId: UInt16 = 1

    func callMethod<T: Decodable>(
        _ method: String,
        params: (some Encodable)?,
        options _: JSONRPCOptions,
        completion closure: ((Result<T, Error>) -> Void)?
    ) throws -> UInt16 {
        let id = nextId
        nextId += 1
        calls[id] = RecordedCall(
            method: method,
            params: params as? JSON,
            completion: { result in closure?(result.map { $0 as! T }) }
        )
        return id
    }

    func subscribe<T: Decodable>(
        _ method: String,
        params: (some Encodable)?,
        unsubscribeMethod: String,
        options: JSONRPCOptions,
        onSubscribed: ((JSONRPCSubscriptionId) -> Void)?,
        updateClosure: @escaping (T) -> Void,
        failureClosure: @escaping (Error, Bool) -> Void
    ) throws -> UInt16 {
        let id = nextId
        nextId += 1
        subscriptions[id] = RecordedSubscription(
            method: method,
            params: params as? JSON,
            unsubscribeMethod: unsubscribeMethod,
            resendOnReconnect: options.resendOnReconnect,
            onSubscribed: onSubscribed,
            update: { updateClosure($0 as! T) },
            failure: failureClosure
        )
        return id
    }

    func cancelForIdentifiers(_ identifiers: [UInt16], sendUnsubscribe: Bool) {
        if sendUnsubscribe {
            cancelled.append(contentsOf: identifiers)
        } else {
            silentlyCancelled.append(contentsOf: identifiers)
        }
    }

    func addBatchCallMethod(
        _: String,
        params _: (some Encodable)?,
        batchId _: JSONRPCBatchId
    ) throws {}

    func submitBatch(
        for _: JSONRPCBatchId,
        options _: JSONRPCOptions,
        completion _: (([Result<JSON, Error>]) -> Void)?
    ) throws -> [UInt16] { [] }

    func clearBatch(for _: JSONRPCBatchId) {}
}

/// MockJSONRPCEngine wearing the full ChainConnection surface, for stubbing
/// ChainRegistryProtocol.getConnection(for:).
final class MockChainConnection: MockJSONRPCEngine, ChainConnection {
    var urls: [URL] = []
    var state: WebSocketEngine.State = .connected(url: URL(string: "wss://mock.node")!)

    func changeUrls(_ newUrls: [URL]) {
        urls = newUrls
    }

    func connect() {}

    func disconnect(_: Bool) {}
}
