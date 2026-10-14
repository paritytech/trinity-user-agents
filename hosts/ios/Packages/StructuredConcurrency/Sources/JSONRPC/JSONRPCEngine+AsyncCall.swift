import Foundation
import SubstrateSdk

private final class CallIdBox {
    var value: UInt16?
}

public extension JSONRPCEngine {
    func asyncCallMethod<R: Decodable>(
        _ method: String,
        params: (some Encodable)?,
        options: JSONRPCOptions
    ) async throws -> R {
        let mutex = NSLock()
        let callIdBox = CallIdBox()

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                mutex.lock()

                defer {
                    mutex.unlock()
                }

                do {
                    callIdBox.value = try self.callMethod(
                        method,
                        params: params,
                        options: options
                    ) { result in
                        mutex.lock()

                        defer {
                            mutex.unlock()
                        }

                        guard callIdBox.value != nil else {
                            return
                        }

                        callIdBox.value = nil

                        continuation.resume(with: result)
                    }
                } catch {
                    callIdBox.value = nil
                    continuation.resume(throwing: error)
                }
            }
        } onCancel: {
            mutex.lock()

            defer {
                mutex.unlock()
            }

            if let cancellationIdentifier = callIdBox.value {
                callIdBox.value = nil
                cancelForIdentifier(cancellationIdentifier)
            }
        }
    }

    func asyncCallVoidMethod(
        _ method: String,
        params: (some Encodable)?,
        options: JSONRPCOptions
    ) async throws {
        // substrate sdk currently requires Decodable response but still handles empty result internally
        let _: String? = try await asyncCallMethod(method, params: params, options: options)
    }
}
