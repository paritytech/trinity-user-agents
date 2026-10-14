import UIKit
import FoundationExt
import PolkadotUI

final class NoSlotsAvailableViewController: UIViewController, ViewHolder {
    typealias RootViewType = NoSlotsAvailableViewLayout

    let presenter: NoSlotsAvailablePresenterProtocol

    init(presenter: NoSlotsAvailablePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = NoSlotsAvailableViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupActions()
        presenter.setup()
    }
}

extension NoSlotsAvailableViewController: NoSlotsAvailableViewProtocol {
    func didReceive(message: String) {
        rootView.setDescription(message)
    }
}

private extension NoSlotsAvailableViewController {
    func setupActions() {
        rootView.dismissButton.addTarget(
            self,
            action: #selector(handleDismiss),
            for: .touchUpInside
        )
    }

    @objc
    func handleDismiss() {
        presenter.dismiss()
    }
}
