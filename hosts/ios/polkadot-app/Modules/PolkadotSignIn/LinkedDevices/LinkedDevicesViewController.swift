import UIKit
import PolkadotUI
import FoundationExt

final class LinkedDevicesViewController: UIViewController, ViewHolder {
    typealias RootViewType = LinkedDevicesViewLayout

    let presenter: LinkedDevicesPresenterProtocol

    private lazy var addBarButtonItem = UIBarButtonItem(
        image: .linkedDevicesAdd,
        style: .plain,
        target: self,
        action: #selector(actionAddDevice)
    )

    init(presenter: LinkedDevicesPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = LinkedDevicesViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        navigationItem.title = String(localized: .linkedDevicesTitle)
        setupHandlers()
        presenter.setup()
    }
}

extension LinkedDevicesViewController: LinkedDevicesViewProtocol {
    func didReceive(viewModel: LinkedDevicesViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
        setupNavigationItems(for: viewModel)
    }
}

private extension LinkedDevicesViewController {
    func setupNavigationItems(for viewModel: LinkedDevicesViewLayout.ViewModel) {
        switch viewModel {
        case .empty:
            navigationItem.rightBarButtonItem = nil
        case .devices:
            navigationItem.rightBarButtonItem = addBarButtonItem
        }
    }

    @objc func actionAddDevice() {
        presenter.scanQRCode()
    }

    func setupHandlers() {
        rootView.bind(scanAction: { [weak self] in
            self?.presenter.scanQRCode()
        })

        rootView.bind(howItWorksAction: { [weak self] in
            self?.presenter.howItWorks()
        })

        rootView.bind(deviceSelectedAction: { [weak self] index in
            self?.presenter.selectDevice(at: index)
        })
    }
}
