import UIKit
import PolkadotUI
import Foundation_iOS
import SwiftUI

final class SelectTokenViewController: UIHostingController<SelectTokenViewLayout> {
    let presenter: SelectTokenPresenterProtocol

    init(presenter: SelectTokenPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: SelectTokenViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        setupHandlers()
        presenter.setup()
    }

    private func setupHandlers() {
        rootView.viewModel.onTap = { [unowned presenter] in
            presenter.select(viewModel: $0)
        }
    }
}

extension SelectTokenViewController: SelectTokenViewProtocol {
    func didReceive(viewModels: [SelectTokenCellViewModel]) {
        rootView.viewModel.viewModels = viewModels
    }
}
