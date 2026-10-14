import Foundation
import AsyncExtensions
import SubstrateStorageSubscription
import SubstrateSdk
import Operation_iOS
import SDKLogger

public extension CallbackBatchStorageSubscription {
    /// Subscribes to `requests` and streams each result.
    ///
    /// Transparently bounded by the node's `state_subscribeStorage` key limit: when `requests`
    /// exceeds ``maxSubscriptionKeys`` the keys are split across several subscriptions. Their initial
    /// values are folded into one map and held back until the map
    /// holds **every** key, so the first emission is always the complete current snapshot — never a
    /// partial chunk — after which each chunk's later notifications pass through as deltas. Within the
    /// limit it is a single native subscription (initial snapshot, then deltas) — identical shape.
    static func asyncStream(
        requests: [BatchStorageSubscriptionRequest],
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        repository: AnyDataProviderRepository<ChainStorageItem>? = nil,
        logger: SDKLoggerProtocol?
    ) -> AnyAsyncSequence<T> {
        guard requests.count > maxSubscriptionKeys else {
            return asyncChunkStream(
                requests: requests,
                connection: connection,
                runtimeService: runtimeService,
                repository: repository,
                logger: logger
            )
        }

        let chunks = stride(from: 0, to: requests.count, by: maxSubscriptionKeys).map { start in
            Array(requests[start ..< Swift.min(start + maxSubscriptionKeys, requests.count)])
        }
        let expectedSize = Set(requests.compactMap(\.mappingKey)).count

        return multiplexedAsyncStream(
            chunks: chunks,
            expectedSize: expectedSize,
            connection: connection,
            runtimeService: runtimeService,
            repository: repository,
            logger: logger
        )
    }

    /// The node's per-`state_subscribeStorage` key cap. Longer request lists are chunked to this size.
    private static var maxSubscriptionKeys: Int { 1_000 }

    /// Merges the chunk subscriptions into one stream by accumulating every key's latest value into a
    /// map and emitting a rebuilt `T` only once the map is complete.
    private static func multiplexedAsyncStream(
        chunks: [[BatchStorageSubscriptionRequest]],
        expectedSize: Int,
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        repository: AnyDataProviderRepository<ChainStorageItem>?,
        logger: SDKLoggerProtocol?
    ) -> AnyAsyncSequence<T> {
        let rawStream = mergedRawSubscriptionStream(
            chunks: chunks,
            connection: connection,
            runtimeService: runtimeService,
            repository: repository,
            logger: logger
        )

        return AsyncThrowingStream<T, Error> { continuation in
            let task = Task {
                do {
                    let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()
                    let context = coderFactory.createRuntimeJsonContext().toRawContext()

                    // Accumulate every chunk's initial values until all keys are known, emit that
                    // complete snapshot once, then forward each chunk's later notifications as-is —
                    // deltas of only the changed keys, matching a single native subscription (initial
                    // complete snapshot, then deltas), which is the shape consumers process.
                    var accumulated: [String: JSON] = [:]
                    var didEmitSnapshot = false

                    for try await raw in rawStream {
                        if didEmitSnapshot {
                            try continuation.yield(
                                T(values: raw.values, blockHashJson: raw.blockHashJson, context: context)
                            )
                            continue
                        }

                        for value in raw.values {
                            guard let mappingKey = value.mappingKey else { continue }
                            accumulated[mappingKey] = value.value
                        }

                        guard accumulated.count == expectedSize else { continue }

                        didEmitSnapshot = true
                        let values = accumulated.map {
                            BatchStorageSubscriptionResultValue(mappingKey: $0.key, value: $0.value)
                        }
                        try continuation.yield(T(values: values, blockHashJson: raw.blockHashJson, context: context))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    /// One subscription over `requests` (assumed within the node's key limit), as an async stream.
    private static func asyncChunkStream(
        requests: [BatchStorageSubscriptionRequest],
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        repository: AnyDataProviderRepository<ChainStorageItem>?,
        logger: SDKLoggerProtocol?
    ) -> AnyAsyncSequence<T> {
        let callbackQueue = DispatchQueue(label: "io.callback.storage.subscription")

        let stream = AsyncThrowingStream<T, Error> { continuation in
            let subscription = CallbackBatchStorageSubscription(
                requests: requests,
                connection: connection,
                runtimeService: runtimeService,
                repository: repository,
                operationQueue: OperationManagerFacade.sharedDefaultQueue,
                callbackQueue: callbackQueue
            ) { result in
                switch result {
                case let .success(value):
                    continuation.yield(value)
                case let .failure(error):
                    continuation.finish(throwing: error)
                }
            }

            continuation.onTermination = { _ in
                logger?.debug("Stream unsubscribed")
                subscription.unsubscribe()
            }

            subscription.subscribe()
        }

        return stream.eraseToAnyAsyncSequence()
    }
}

/// Merges the raw per-chunk subscription streams into one, forwarding every emission. Free function so
/// it is fixed to ``BatchStorageSubscriptionRawResult`` regardless of the caller's result type.
private func mergedRawSubscriptionStream(
    chunks: [[BatchStorageSubscriptionRequest]],
    connection: JSONRPCEngine,
    runtimeService: RuntimeCodingServiceProtocol,
    repository: AnyDataProviderRepository<ChainStorageItem>?,
    logger: SDKLoggerProtocol?
) -> AnyAsyncSequence<BatchStorageSubscriptionRawResult> {
    AsyncThrowingStream<BatchStorageSubscriptionRawResult, Error> { continuation in
        let tasks = chunks.map { chunk in
            Task {
                do {
                    let stream = CallbackBatchStorageSubscription<BatchStorageSubscriptionRawResult>.asyncStream(
                        requests: chunk,
                        connection: connection,
                        runtimeService: runtimeService,
                        repository: repository,
                        logger: logger
                    )
                    for try await raw in stream {
                        continuation.yield(raw)
                    }
                } catch {
                    continuation.finish(throwing: error)
                }
            }
        }
        continuation.onTermination = { _ in tasks.forEach { $0.cancel() } }
    }
    .eraseToAnyAsyncSequence()
}
