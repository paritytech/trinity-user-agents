import Foundation
import SubstrateSdk
import AsyncExtensions

public extension JSONRPCEngine {
    func asyncSubscribe<T: Decodable>(
        _ method: String,
        params: (some Encodable)?,
        unsubscribeMethod: String
    ) -> AnyAsyncSequence<T> {
        let stream = AsyncThrowingStream { continuation in
            let updateClosure: (T) -> Void = { update in
                continuation.yield(update)
            }

            let failureClosure: (Error, Bool) -> Void = { error, unsubscribed in
                if unsubscribed {
                    continuation.finish(throwing: error)
                } else {
                    continuation.yield(with: .failure(error))
                }
            }

            let idHolder = AnyObjectHolder<UInt16>()

            continuation.onTermination = { _ in
                if let subscriptionId = idHolder.get() {
                    idHolder.set(nil)
                    self.cancelForIdentifier(subscriptionId)
                }
            }

            do {
                let subscriptionId = try self.subscribe(
                    method,
                    params: params,
                    unsubscribeMethod: unsubscribeMethod,
                    updateClosure: updateClosure,
                    failureClosure: failureClosure
                )

                idHolder.set(subscriptionId)
            } catch {
                continuation.finish(throwing: error)
            }
        }

        return stream.eraseToAnyAsyncSequence()
    }

    func asyncSubscribe<T: Decodable>(
        _ method: String,
        unsubscribeMethod: String
    ) -> AnyAsyncSequence<T> {
        let params: EmptyParam? = nil
        return asyncSubscribe(
            method,
            params: params,
            unsubscribeMethod: unsubscribeMethod
        )
        .eraseToAnyAsyncSequence()
    }
}

private struct EmptyParam: Encodable {}
