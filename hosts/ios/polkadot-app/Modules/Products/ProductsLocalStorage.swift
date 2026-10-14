import Foundation
import Keystore_iOS
import Products
import AsyncExtensions

final class ProductsLocalStorage: ProductLocalStorageProtocol, @unchecked Sendable {
    private static let keyPrefix = "io.polkadotapp.ProductStorage"

    private let productId: String
    private let settingsManager: SettingsManagerProtocol
    private let lock = NSLock()
    private var continuations: [String: [UUID: AsyncStream<String?>.Continuation]] = [:]

    init(productId: String, settingsManager: SettingsManagerProtocol) {
        self.productId = productId
        self.settingsManager = settingsManager
    }

    func read(key: String) async -> String? {
        settingsManager.string(for: storageKey(key))
    }

    func write(key: String, value: String) async {
        // Mutate and notify in one critical section so a subscribe racing this
        // write cannot read the new value as its initial emit and then receive
        // it again from the notification.
        lock.withLock {
            settingsManager.set(value: value, for: storageKey(key))
            continuations[key]?.values.forEach { $0.yield(value) }
        }
    }

    func clear(key: String) async {
        lock.withLock {
            settingsManager.removeValue(for: storageKey(key))
            continuations[key]?.values.forEach { $0.yield(nil) }
        }
    }

    func subscribe(key: String) -> AnyAsyncSequence<String?> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<String?>.makeStream(bufferingPolicy: .unbounded)

        // Register and emit the current value under the lock so a concurrent
        // write/clear cannot slip its notification ahead of the initial value.
        lock.withLock {
            continuations[key, default: [:]][id] = continuation
            continuation.yield(settingsManager.string(for: storageKey(key)))
        }

        // Set after registering: if the stream already terminated, Foundation
        // invokes this immediately, so the just-registered continuation is still
        // cleaned up rather than leaking in `continuations`.
        continuation.onTermination = { [weak self] _ in
            self?.removeContinuation(key: key, id: id)
        }

        return stream.eraseToAnyAsyncSequence()
    }

    private func removeContinuation(key: String, id: UUID) {
        lock.withLock {
            continuations[key]?[id] = nil
            if continuations[key]?.isEmpty == true {
                continuations[key] = nil
            }
        }
    }

    private func storageKey(_ key: String) -> String {
        "\(Self.keyPrefix).\(productId).\(key)"
    }
}
