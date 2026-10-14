import AsyncExtensions
import Foundation
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
        branding.send(PaymentAssetBrand(symbol: "USD", squareIcon: nil, wideIcon: nil))
        try await Task.sleep(for: .milliseconds(100))

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

    final class StubBranding: PaymentAssetBrandingProviding {
        private let subject: AsyncCurrentValueSubject<PaymentAssetBrand>

        init(initial: PaymentAssetBrand) {
            subject = AsyncCurrentValueSubject(initial)
        }

        var current: PaymentAssetBrand { subject.value }

        func stream() -> AnyAsyncSequence<PaymentAssetBrand> {
            subject.eraseToAnyAsyncSequence()
        }

        func send(_ brand: PaymentAssetBrand) {
            subject.send(brand)
        }
    }
}
