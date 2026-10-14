import UIKit
import PolkadotUI
import SwiftUI

@MainActor
enum CurrencyPickerViewFactory {
    static func createView(
        selectedCurrencyManager: SelectedCurrencyManaging = SelectedCurrencyManager.shared
    ) -> UIViewController {
        let viewModel = CurrencyPickerViewModel(
            currencies: Currency.supported,
            selectedCode: selectedCurrencyManager.selectedCurrency.code,
            coingeckoOperationFactory: CoingeckoOperationFactory()
        )

        viewModel.onSelect = { currency in
            selectedCurrencyManager.save(currency: currency)
        }

        let layout = CurrencyPickerViewLayout(viewModel: viewModel)
        let hostingController = UIHostingController(rootView: layout)
        hostingController.view.backgroundColor = .bgSurfaceMain

        return hostingController
    }
}
