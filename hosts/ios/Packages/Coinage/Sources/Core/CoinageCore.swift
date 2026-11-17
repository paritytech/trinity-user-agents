import BigInt
import Foundation
import TrUAPIHost

public enum CoinageCoreError: Swift.Error {
    /// The core reported an asset unit that is not a number.
    case invalidAssetUnit(String)
}

/// The core's coinage layer, which holds the wallet, reached through the shared runtime.
///
/// `handle` waits until the layer has started and fails with the reason it does not run; every call
/// here goes through it, so nothing reaches the layer before it accepts operations.
public final class CoinageCore: Sendable {
    public typealias Handle = @Sendable () async throws -> NativeCoinage

    private let handle: Handle
    private let precision: Int16

    /// `precision` is the payment asset's decimal precision, which the core's cents are rendered in.
    public init(precision: Int16, handle: @escaping Handle) {
        self.precision = precision
        self.handle = handle
    }

    public func use<T>(_ body: (NativeCoinage) async throws -> T) async throws -> T {
        try await body(handle())
    }

    public func units() async throws -> CoinageUnits {
        let assetUnit = try await handle().assetUnit()
        guard let unit = BigUInt(assetUnit) else {
            throw CoinageCoreError.invalidAssetUnit(assetUnit)
        }
        return CoinageUnits(unit: unit, precision: precision)
    }

    /// Subscribes with `open`, which hands the core an observer that pushes into the returned stream;
    /// the subscription is cancelled when the stream terminates.
    public func observe<Item: Sendable>(
        _ open: @escaping @Sendable (NativeCoinage, @escaping @Sendable (Item) -> Void) async throws
            -> NativeSubscription
    ) -> AsyncThrowingStream<Item, Swift.Error> {
        AsyncThrowingStream { continuation in
            let holder = SubscriptionHolder()
            let task = Task { [handle] in
                do {
                    let subscription = try await open(handle()) { continuation.yield($0) }
                    holder.hold(subscription)
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in
                task.cancel()
                holder.release()
            }
        }
    }
}

/// Keeps a subscription until the stream it feeds ends, including one that arrives after the end.
private final class SubscriptionHolder: @unchecked Sendable {
    private let lock = NSLock()
    private var subscription: NativeSubscription?
    private var isReleased = false

    func hold(_ newSubscription: NativeSubscription) {
        lock.lock()
        let released = isReleased
        if !released {
            subscription = newSubscription
        }
        lock.unlock()

        if released {
            newSubscription.cancel()
        }
    }

    func release() {
        lock.lock()
        isReleased = true
        let current = subscription
        subscription = nil
        lock.unlock()

        current?.cancel()
    }
}
