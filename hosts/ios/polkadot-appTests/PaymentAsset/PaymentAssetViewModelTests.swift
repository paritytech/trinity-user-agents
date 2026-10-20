import AsyncExtensions
import Foundation
import os
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import UIKit

@testable import polkadot_app

@Suite("Payment asset view model")
@MainActor
struct PaymentAssetViewModelTests {
    @Test("Binding replays the current brand and every later one")
    func bindDeliversBrands() async throws {
        let branding = StubBranding(initial: PaymentAssetBrand(symbol: "CASH", squareIcon: nil, wideIcon: nil))
        let viewModel = PaymentAssetViewModel(branding: branding)
        let received = Received()

        viewModel.bind { brand in
            received.symbols.append(brand.symbol)
        }

        try await received.wait(untilCount: 1)
        branding.send(PaymentAssetBrand(symbol: "USD", squareIcon: nil, wideIcon: nil))
        try await received.wait(untilCount: 2)

        #expect(received.symbols == ["CASH", "USD"])
    }

    @Test("Cancel stops further deliveries")
    func cancelStopsDeliveries() async throws {
        let branding = StubBranding(initial: PaymentAssetBrand(symbol: "CASH", squareIcon: nil, wideIcon: nil))
        let viewModel = PaymentAssetViewModel(branding: branding)
        let received = Received()

        viewModel.bind { brand in
            received.symbols.append(brand.symbol)
        }
        try await received.wait(untilCount: 1)

        viewModel.cancel()
        try await branding.waitForSubscriptionToEnd()
        branding.send(PaymentAssetBrand(symbol: "USD", squareIcon: nil, wideIcon: nil))

        #expect(received.symbols == ["CASH"])
    }

    @Test("The factory hands out independent view models")
    func factoryMakesIndependentViewModels() {
        let factory = PaymentAssetViewModelFactory(branding: StubBranding(
            initial: PaymentAssetBrand(symbol: "CASH", squareIcon: nil, wideIcon: nil)
        ))

        #expect(factory.makeViewModel() !== factory.makeViewModel())
    }
}

private extension PaymentAssetViewModelTests {
    @MainActor
    final class Received {
        var symbols: [String] = []

        func wait(untilCount count: Int) async throws {
            for _ in 0 ..< 50 where symbols.count < count {
                try await Task.sleep(for: .milliseconds(20))
            }
            #expect(symbols.count >= count)
        }
    }

    final class StubBranding: PaymentAssetBrandingProviding, @unchecked Sendable {
        private let state: OSAllocatedUnfairLock<
            (current: PaymentAssetBrand, subscribers: [UUID: AsyncStream<PaymentAssetBrand>.Continuation])
        >
        private let ended = AsyncStream<Void>.makeStream()

        init(initial: PaymentAssetBrand) {
            state = .init(initialState: (initial, [:]))
        }

        var current: PaymentAssetBrand { state.withLock { $0.current } }

        func stream() -> AnyAsyncSequence<PaymentAssetBrand> {
            let id = UUID()
            let (stream, continuation) = AsyncStream<PaymentAssetBrand>.makeStream()
            continuation.onTermination = { [state, ended] _ in
                state.withLock { _ = $0.subscribers.removeValue(forKey: id) }
                ended.continuation.yield()
            }
            state.withLock { state in
                state.subscribers[id] = continuation
                continuation.yield(state.current)
            }
            return stream.eraseToAnyAsyncSequence()
        }

        func send(_ brand: PaymentAssetBrand) {
            let subscribers = state.withLock { state in
                state.current = brand
                return Array(state.subscribers.values)
            }
            subscribers.forEach { $0.yield(brand) }
        }

        /// Returns once a subscriber's stream has ended, failing rather than hanging if none does.
        func waitForSubscriptionToEnd() async throws {
            try await withTimeout(.hangGuard) { [ended] in
                for await _ in ended.stream {
                    return
                }
                throw CancellationError()
            }
        }
    }
}
