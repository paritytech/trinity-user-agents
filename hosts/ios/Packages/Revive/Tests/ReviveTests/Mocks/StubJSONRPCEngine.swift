import Foundation
import SubstrateSdk

/// A connection that is never reached: the tests stop at the request factory.
final class StubJSONRPCEngine: JSONRPCEngine {
    func callMethod<T: Decodable>(
        _: String,
        params _: (some Encodable)?,
        options _: JSONRPCOptions,
        completion _: ((Result<T, Error>) -> Void)?
    ) throws -> UInt16 {
        throw StubError.unused
    }

    func subscribe<T: Decodable>(
        _: String,
        params _: (some Encodable)?,
        unsubscribeMethod _: String,
        options _: JSONRPCOptions,
        onSubscribed _: ((JSONRPCSubscriptionId) -> Void)?,
        updateClosure _: @escaping (T) -> Void,
        failureClosure _: @escaping (Error, Bool) -> Void
    ) throws -> UInt16 {
        throw StubError.unused
    }

    func cancelForIdentifiers(_: [UInt16], sendUnsubscribe _: Bool) {}

    func addBatchCallMethod(_: String, params _: (some Encodable)?, batchId _: JSONRPCBatchId) throws {
        throw StubError.unused
    }

    func submitBatch(
        for _: JSONRPCBatchId,
        options _: JSONRPCOptions,
        completion _: (([Result<JSON, Error>]) -> Void)?
    ) throws -> [UInt16] {
        throw StubError.unused
    }

    func clearBatch(for _: JSONRPCBatchId) {}
}
