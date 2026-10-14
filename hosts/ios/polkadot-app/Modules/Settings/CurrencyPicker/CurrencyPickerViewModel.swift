import Foundation
import Observation

@Observable
final class CurrencyPickerViewModel {
    var searchText: String = ""
    var selectedCode: String
    var loadingCode: String?
    var showRateLimitError = false

    let currencies: [Currency]
    var onSelect: ((Currency) -> Void)?

    private let coingeckoOperationFactory: CoingeckoOperationFactoryProtocol

    init(
        currencies: [Currency],
        selectedCode: String,
        coingeckoOperationFactory: CoingeckoOperationFactoryProtocol
    ) {
        self.currencies = currencies
        self.selectedCode = selectedCode
        self.coingeckoOperationFactory = coingeckoOperationFactory
    }

    var filteredCurrencies: [Currency] {
        guard !searchText.isEmpty else {
            return currencies
        }

        let query = searchText.lowercased()
        return currencies.filter {
            $0.code.lowercased().contains(query) ||
                $0.name.lowercased().contains(query)
        }
    }

    func select(_ currency: Currency) async -> Bool {
        guard currency.code != selectedCode else {
            return true
        }

        loadingCode = currency.code
        defer { loadingCode = nil }

        let success = await fetchPrice(for: currency)

        guard success else {
            showRateLimitError = true
            return false
        }

        selectedCode = currency.code
        onSelect?(currency)
        return true
    }
}

// MARK: - Private functions

extension CurrencyPickerViewModel {
    private static let priceCheckTokenIds = ["tether"]

    private func fetchPrice(for currency: Currency) async -> Bool {
        let operation = coingeckoOperationFactory.fetchPriceOperation(
            for: Self.priceCheckTokenIds,
            currency: currency
        )

        do {
            _ = try await operation.asyncExecute()
            return true
        } catch {
            return false
        }
    }
}
