import Foundation
import TrUAPIHost

/// The core's streams, each as an `AsyncThrowingStream` that ends the subscription when it terminates.
public extension CoinageCore {
    func balances() -> AsyncThrowingStream<Balance, Swift.Error> {
        observe { coinage, push in
            try await coinage.observeBalance(observer: BalanceObserver(push: push))
        }
    }

    func holdings() -> AsyncThrowingStream<Holdings, Swift.Error> {
        observe { coinage, push in
            try await coinage.observeHoldings(observer: HoldingsObserver(push: push))
        }
    }

    /// The status of the app's operation `id`, until the stream is cancelled.
    func statuses(of id: CoinageOperationId) -> AsyncThrowingStream<StatusItem, Swift.Error> {
        observe { coinage, push in
            try await coinage.observeStatus(id: id.value, observer: StatusObserver(push: push))
        }
    }

    func exportStatuses(of id: CoinageOperationId) -> AsyncThrowingStream<[ExportedCoinStatus], Swift.Error> {
        observe { coinage, push in
            try await coinage.observeExportStatus(id: id.value, observer: ExportStatusObserver(push: push))
        }
    }

    func events() -> AsyncThrowingStream<Event, Swift.Error> {
        observe { coinage, push in
            try await coinage.observeEvents(observer: EventObserver(push: push))
        }
    }

    func handOverProgress() -> AsyncThrowingStream<HandOverProgress, Swift.Error> {
        observe { coinage, push in
            coinage.observeHandOver(observer: HandOverObserver(push: push))
        }
    }
}

private final class BalanceObserver: NativeBalanceObserver {
    let push: @Sendable (Balance) -> Void

    init(push: @escaping @Sendable (Balance) -> Void) {
        self.push = push
    }

    func onBalance(balance: Balance) {
        push(balance)
    }
}

private final class HoldingsObserver: NativeHoldingsObserver {
    let push: @Sendable (Holdings) -> Void

    init(push: @escaping @Sendable (Holdings) -> Void) {
        self.push = push
    }

    func onHoldings(holdings: Holdings) {
        push(holdings)
    }
}

private final class StatusObserver: NativeStatusObserver {
    let push: @Sendable (StatusItem) -> Void

    init(push: @escaping @Sendable (StatusItem) -> Void) {
        self.push = push
    }

    func onStatus(item: StatusItem) {
        push(item)
    }
}

private final class ExportStatusObserver: NativeExportStatusObserver {
    let push: @Sendable ([ExportedCoinStatus]) -> Void

    init(push: @escaping @Sendable ([ExportedCoinStatus]) -> Void) {
        self.push = push
    }

    func onExportStatus(coins: [ExportedCoinStatus]) {
        push(coins)
    }
}

private final class EventObserver: NativeEventObserver {
    let push: @Sendable (Event) -> Void

    init(push: @escaping @Sendable (Event) -> Void) {
        self.push = push
    }

    func onEvent(event: Event) {
        push(event)
    }
}

private final class HandOverObserver: NativeHandOverObserver {
    let push: @Sendable (HandOverProgress) -> Void

    init(push: @escaping @Sendable (HandOverProgress) -> Void) {
        self.push = push
    }

    func onProgress(progress: HandOverProgress) {
        push(progress)
    }
}
