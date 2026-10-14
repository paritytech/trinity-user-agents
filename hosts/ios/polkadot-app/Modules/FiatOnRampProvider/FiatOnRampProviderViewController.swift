import Foundation_iOS
import PolkadotUI
import SwiftUI
import UIKit

final class FiatOnRampProviderViewController: UIHostingController<FiatOnRampProviderViewLayout> {
    let presenter: FiatOnRampProviderPresenterProtocol
    private let viewModel = FiatOnRampProviderViewModel()

    init(presenter: FiatOnRampProviderPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: FiatOnRampProviderViewLayout(viewModel: viewModel))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        setupNavigation()
        setupHandlers()
        presenter.setup()
    }

    private func setupNavigation() {
        navigationItem.title = ""
    }

    private func setupHandlers() {
        viewModel.onSelect = { [unowned presenter] model in
            presenter.select(provider: model)
        }
        viewModel.onConfirmOpenUrl = { [weak presenter] url in
            presenter?.openWidget(url: url)
        }
    }
}

extension FiatOnRampProviderViewController: FiatOnRampProviderViewProtocol {
    func didReceive(viewModels: [FiatOnRampProviderItemViewModel]) {
        viewModel.viewModels = viewModels
    }

    func didReceive(isLoading: Bool) {
        viewModel.isLoading = isLoading
    }

    func didReceive(isWidgetLoading: Bool) {
        viewModel.isWidgetLoading = isWidgetLoading
    }

    func didReceive(isRefreshing: Bool) {
        viewModel.isRefreshing = isRefreshing
    }

    func didReceive(refreshCountdownText: String?) {
        viewModel.refreshCountdownText = refreshCountdownText
    }

    func didReceive(confirmUrl: URL) {
        viewModel.confirmation = FiatOnRampProviderConfirmation(url: confirmUrl)
    }
}
