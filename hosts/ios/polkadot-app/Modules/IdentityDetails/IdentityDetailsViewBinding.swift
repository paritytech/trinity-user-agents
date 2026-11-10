import Foundation
import SwiftUI
import UIKit
import UIKitExt
import PolkadotUI

final class IdentityDetailsViewBinding: IdentityDetailsViewProtocol {
    let viewModel: IdentityDetailsViewModel
    weak var navigationHost: ControllerBackedProtocol?

    init(viewModel: IdentityDetailsViewModel) {
        self.viewModel = viewModel
    }

    var isSetup: Bool {
        navigationHost?.isSetup ?? false
    }

    var controller: UIViewController {
        guard let controller = navigationHost?.controller else {
            assertionFailure("IdentityDetailsViewBinding requires a navigation host before presenting UI")
            return UIViewController()
        }

        return controller
    }

    func bind(to presenter: IdentityDetailsPresenterProtocol) {
        viewModel.onCopy = { [weak presenter] in
            presenter?.onCopyUsername()
        }

        viewModel.onShare = { [weak presenter] in
            presenter?.onShare()
        }

        viewModel.onQrCode = { [weak presenter] in
            presenter?.onQrCode()
        }
    }

    func didReceive(username: Username, claimed: Bool) {
        viewModel.username = .init(value: username.value, isClaimed: claimed)
    }

    func didReceive(qrCode: UIImage) {
        viewModel.qrCode = Image(uiImage: qrCode)
    }
}
