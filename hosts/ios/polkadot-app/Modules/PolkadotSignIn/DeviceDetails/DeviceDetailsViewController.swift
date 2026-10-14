import UIKit
import PolkadotUI
import FoundationExt

final class DeviceDetailsViewController: UIViewController, ViewHolder {
    typealias RootViewType = DeviceDetailsViewLayout

    let presenter: DeviceDetailsPresenterProtocol

    init(presenter: DeviceDetailsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = DeviceDetailsViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        navigationItem.title = String(localized: .linkedDevicesDeviceDetailsTitle)
        setupHandlers()
        presenter.setup()
    }
}

extension DeviceDetailsViewController: DeviceDetailsViewProtocol {
    func didReceive(viewModel: DeviceDetailsViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}

private extension DeviceDetailsViewController {
    func setupHandlers() {
        rootView.bind(removeAction: { [weak self] in
            self?.presenter.removeDevice()
        })
    }
}
