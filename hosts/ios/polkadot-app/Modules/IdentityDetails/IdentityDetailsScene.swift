import Foundation
import UIKitExt
import PolkadotUI

@MainActor
final class IdentityDetailsScene {
    let viewModel: IdentityDetailsViewModel

    private let binding: IdentityDetailsViewBinding
    private let presenter: IdentityDetailsPresenter
    private let interactor: IdentityDetailsInteractor
    private var didSetup = false

    init(
        viewModel: IdentityDetailsViewModel,
        binding: IdentityDetailsViewBinding,
        presenter: IdentityDetailsPresenter,
        interactor: IdentityDetailsInteractor
    ) {
        self.viewModel = viewModel
        self.binding = binding
        self.presenter = presenter
        self.interactor = interactor
    }

    func attachNavigationHost(_ navigationHost: ControllerBackedProtocol) {
        binding.navigationHost = navigationHost
    }

    func setup() {
        guard !didSetup else {
            return
        }

        didSetup = true
        presenter.setup()
    }

    func share() {
        presenter.onShare()
    }
}
