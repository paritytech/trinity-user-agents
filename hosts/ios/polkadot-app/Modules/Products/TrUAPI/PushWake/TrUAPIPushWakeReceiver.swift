import Foundation
import TrUAPIHost
import UIKit

/// Gives the wake pushes of the push relay to the core (VERBOSITY-WASM-PRODUCT A1, 07-LATER §10 W3).
///
/// A wake push is a background push with the key `truapi-wake`: a statement-store topic, a statement hash, a send
/// time and a MAC. It carries no message content. The core checks it and wakes each product that watches the topic
/// for 30 seconds. The worker of the product then reads the statement store and shows what came, for example as a
/// local notification. The app stays active until the end of each hold.
@MainActor
final class TrUAPIPushWakeReceiver {
    static let shared = TrUAPIPushWakeReceiver()

    /// The longest time the app keeps a background fetch open for a wake. iOS gives about 30 seconds.
    static let longestFetch: TimeInterval = 28

    private let workers: @MainActor () -> TrUAPIWorkerManaging?
    private let now: () -> Date
    private let logger: LoggerProtocol

    init(
        workers: @escaping @MainActor () -> TrUAPIWorkerManaging? = { ProductPocketService.current?.workers },
        now: @escaping () -> Date = Date.init,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.workers = workers
        self.now = now
        self.logger = logger
    }

    /// The JSON text of the wake push in `userInfo`, or nil when the push is not a wake push.
    nonisolated static func payload(of userInfo: [AnyHashable: Any]) -> String? {
        guard
            let wake = userInfo[wakePushKey],
            JSONSerialization.isValidJSONObject(wake),
            let data = try? JSONSerialization.data(withJSONObject: wake, options: [.sortedKeys])
        else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    /// Handles a wake push. Gives false, and does not call `completion`, for any other push.
    func receive(_ userInfo: [AnyHashable: Any], completion: @escaping (UIBackgroundFetchResult) -> Void) -> Bool {
        guard let payload = Self.payload(of: userInfo) else { return false }

        Task {
            let result = await self.handle(payload)
            completion(result)
        }
        return true
    }

    /// Gives the push to the core and waits until the last hold ends, at most ``longestFetch``.
    func handle(_ payload: String) async -> UIBackgroundFetchResult {
        logger.info("[truapi] wake push received")
        guard let workers = workers() else {
            // A launch in the background connects no scene, so the session that builds the core does not exist.
            logger.warning("[truapi] wake push before the TrUAPI session exists: nothing woken")
            return .noData
        }

        let deliveries: [PushWakeDelivery]
        do {
            deliveries = try await workers.receiveWakePush(payload: payload)
        } catch {
            logger.warning("[truapi] wake push refused: \(error)")
            return .failed
        }

        let ends = deliveries.compactMap { delivery -> Date? in
            guard case let .held(untilMs) = delivery.outcome else { return nil }
            return Date(timeIntervalSince1970: TimeInterval(untilMs) / 1_000)
        }
        guard let end = ends.max() else { return .noData }

        let wait = min(max(end.timeIntervalSince(now()), 0), Self.longestFetch)
        try? await Task.sleep(for: .milliseconds(Int(wait * 1_000)))
        return .newData
    }
}

/// The key of the wake push in a push payload; the core reads the same key (`WAKE_PUSH_PAYLOAD_KEY`).
let wakePushKey = "truapi-wake"
