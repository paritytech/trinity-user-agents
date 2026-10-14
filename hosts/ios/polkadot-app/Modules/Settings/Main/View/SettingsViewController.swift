import UIKit
import PolkadotUI
import SwiftUI

final class SettingsViewController: UIHostingController<SettingsViewLayout>, RootScreen {
    let presenter: SettingsPresenterProtocol

    init(presenter: SettingsPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: SettingsViewLayout(sections: [], appVersion: nil))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        setTitle(String(localized: .settingsMainTitle))
        presenter.setup()
    }
}

extension SettingsViewController: SettingsViewProtocol {
    func applyContent(_ content: SettingsViewModel.Content) {
        rootView = SettingsViewLayout(
            sections: content.sections,
            appVersion: content.appVersion
        )
    }
}
