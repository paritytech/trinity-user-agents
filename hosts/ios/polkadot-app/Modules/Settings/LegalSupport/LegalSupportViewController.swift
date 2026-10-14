import PolkadotUI
import SwiftUI
import UIKit

final class LegalSupportViewController: UIHostingController<SettingsViewLayout> {
    let presenter: LegalSupportPresenterProtocol

    init(presenter: LegalSupportPresenterProtocol) {
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
        title = String(localized: .settingsCellLegalSupport)

        presenter.setup()
    }
}

// MARK: - LegalSupportViewProtocol

extension LegalSupportViewController: LegalSupportViewProtocol {
    func applyContent(_ sections: [SettingsViewLayout.Section]) {
        rootView = SettingsViewLayout(sections: sections, appVersion: nil)
    }
}
