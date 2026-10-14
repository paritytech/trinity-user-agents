import Foundation
import UIKitExt
import PolkadotUI

@MainActor
final class AssetDetailsScene {
    let viewModel: AssetDetailsViewModel

    private let binding: AssetDetailsViewBinding
    private let presenter: AssetDetailsPresenter
    private let interactor: AssetDetailsInteractor
    private var didSetup = false

    init(
        viewModel: AssetDetailsViewModel,
        binding: AssetDetailsViewBinding,
        presenter: AssetDetailsPresenter,
        interactor: AssetDetailsInteractor
    ) {
        self.viewModel = viewModel
        self.binding = binding
        self.presenter = presenter
        self.interactor = interactor
    }

    func attachNavigationHost(_ navigationHost: ControllerBackedProtocol) {
        binding.navigationHost = navigationHost
    }

    func setBackupNotificationAnimationsEnabled(_ isEnabled: Bool) {
        binding.animatesBackupNotificationUpdates = isEnabled
    }

    func setup() {
        guard !didSetup else {
            return
        }

        didSetup = true
        presenter.setup()
    }
}
