import UIKit
import SwiftUI
import PolkadotUI

final class AppPermissionsViewController: UIHostingController<AppPermissionsViewLayout> {
    let presenter: AppPermissionsPresenterProtocol

    init(presenter: AppPermissionsPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: AppPermissionsViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain

        rootView.viewModel.onToggle = { [weak self] item, isOn in
            self?.presenter.toggle(item, isOn: isOn)
        }

        presenter.setup()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        presenter.viewWillDisappear()
    }
}

extension AppPermissionsViewController: AppPermissionsViewProtocol {
    func didReceive(items: [AppPermissionsViewLayout.Item]) {
        rootView.viewModel.items = items
    }

    func setTitle(_ title: String) {
        self.title = title
    }
}
