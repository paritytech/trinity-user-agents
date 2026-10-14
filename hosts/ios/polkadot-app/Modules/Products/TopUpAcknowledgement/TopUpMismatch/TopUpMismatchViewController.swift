import PolkadotUI
import SwiftUI
import UIKit
import UIKit_iOS

final class TopUpMismatchViewController: UIHostingController<TopUpMismatchViewLayout> {
    let presenter: TopUpMismatchPresenterProtocol

    init(presenter: TopUpMismatchPresenterProtocol) {
        self.presenter = presenter
        let placeholder = TopUpMismatchViewModel()
        super.init(rootView: TopUpMismatchViewLayout(viewModel: placeholder))
        isModalInPresentation = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
        presenter.setup()
    }
}

extension TopUpMismatchViewController: TopUpMismatchViewProtocol {
    func didReceive(viewModel: any TopUpMismatchViewModelProtocol) {
        rootView = TopUpMismatchViewLayout(viewModel: viewModel)
    }
}

extension TopUpMismatchViewController: ModalPresenterDelegate {
    func presenterShouldHide(_: any ModalPresenterProtocol) -> Bool {
        false
    }
}
