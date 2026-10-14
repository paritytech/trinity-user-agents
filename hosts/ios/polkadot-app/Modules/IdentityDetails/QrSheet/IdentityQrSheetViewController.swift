import UIKit
import SwiftUI
import PolkadotUI

final class IdentityQrSheetViewController: UIHostingController<IdentityQrSheetView> {
    let presenter: IdentityQrSheetPresenterProtocol
    let viewModel: IdentityDetailsViewModel

    init(presenter: IdentityQrSheetPresenterProtocol, viewModel: IdentityDetailsViewModel) {
        self.presenter = presenter
        self.viewModel = viewModel

        let rootView = IdentityQrSheetView(viewModel: viewModel) { [weak presenter] in
            presenter?.close()
        }

        super.init(rootView: rootView)
        view.backgroundColor = .clear
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        presenter.setup()
    }
}

extension IdentityQrSheetViewController: IdentityQrSheetViewProtocol {}
