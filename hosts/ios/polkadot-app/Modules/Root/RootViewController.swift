import UIKit
import SwiftUI

final class RootViewController: UIHostingController<RootViewLayout> {
    let presenter: RootPresenterProtocol

    init(presenter: RootPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: RootViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        setupHandlers()
    }
}

private extension RootViewController {
    func setupHandlers() {
        rootView.onRetry = { [weak presenter] in
            presenter?.retry()
        }
    }
}

extension RootViewController: RootViewProtocol {
    func didReceive(viewModel: RootViewLayout.ViewModel) {
        rootView.viewModel = viewModel
    }
}
