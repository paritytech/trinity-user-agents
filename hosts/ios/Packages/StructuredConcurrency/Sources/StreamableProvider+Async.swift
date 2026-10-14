import Foundation
import Operation_iOS
import AsyncExtensions

public extension StreamableProviderProtocol {
    func asyncStream(
        with options: StreamableProviderObserverOptions = .allNonblocking()
    ) -> AnyAsyncSequence<[DataProviderChange<Model>]> {
        let syncQueue = DispatchQueue(label: "io.streamable.provider.async.updates")
        let observer = NSObject()

        return AsyncThrowingStream { continuation in
            addObserver(
                observer,
                deliverOn: syncQueue,
                executing: { changes in
                    continuation.yield(changes)
                },
                failing: { error in
                    continuation.finish(throwing: error)
                },
                options: options
            )

            continuation.onTermination = { _ in
                self.removeObserver(observer)
            }
        }
        .eraseToAnyAsyncSequence()
    }

    func asyncLastChangeStream(
        with options: StreamableProviderObserverOptions = .allNonblocking()
    ) -> AnyAsyncSequence<Model?> {
        asyncStream(with: options)
            .map { $0.reduceToLastChange() }
            .eraseToAnyAsyncSequence()
    }
}

public extension StreamableProviderObserverOptions {
    static func allNonblocking() -> StreamableProviderObserverOptions {
        StreamableProviderObserverOptions(
            alwaysNotifyOnRefresh: false,
            waitsInProgressSyncOnAdd: false,
            initialSize: 0,
            refreshWhenEmpty: true
        )
    }
}

private extension Array {
    func reduceToLastChange<T>() -> T? where Element == DataProviderChange<T> {
        reduce(nil) { _, item in
            switch item {
            case let .insert(newItem),
                 let .update(newItem):
                newItem
            case .delete:
                nil
            }
        }
    }
}
