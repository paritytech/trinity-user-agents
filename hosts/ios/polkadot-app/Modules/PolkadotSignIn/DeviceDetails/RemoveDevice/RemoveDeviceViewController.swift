import UIKit
import UIKit_iOS
import FoundationExt
import PolkadotUI

final class RemoveDeviceViewController: UIViewController, ViewHolder {
    typealias RootViewType = RemoveDeviceViewLayout

    let presenter: RemoveDevicePresenterProtocol

    private var isLoading = false

    init(presenter: RemoveDevicePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RemoveDeviceViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupActions()
        presenter.setup()
    }
}

extension RemoveDeviceViewController: RemoveDeviceViewProtocol {
    func didReceive(deviceDescription: String) {
        rootView.bind(deviceDescription: deviceDescription)
    }

    func didReceive(isLoading: Bool) {
        self.isLoading = isLoading
        rootView.setLoading(isLoading)
    }
}

extension RemoveDeviceViewController: ModalSheetPresenterDelegate {
    func presenterShouldHide(_: any ModalPresenterProtocol) -> Bool {
        !isLoading
    }

    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        !isLoading
    }
}

private extension RemoveDeviceViewController {
    func setupActions() {
        rootView.removeButton.addTarget(self, action: #selector(handleRemove), for: .touchUpInside)
        rootView.cancelButton.addTarget(self, action: #selector(handleCancel), for: .touchUpInside)
    }

    @objc
    func handleRemove() {
        presenter.confirm()
    }

    @objc
    func handleCancel() {
        presenter.cancel()
    }
}
