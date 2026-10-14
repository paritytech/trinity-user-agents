import UIKit
import PolkadotUI
import FoundationExt
import UIKit_iOS

final class ShareViewController: UIViewController, ViewHolder {
    typealias RootViewType = ShareViewLayout

    let presenter: SharePresenterProtocol

    init(presenter: SharePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ShareViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        wireHandlers()
        presenter.setup()
    }
}

extension ShareViewController: ShareViewProtocol {
    func didReceive(viewModel: ShareViewLayout.ViewModel) {
        updateBottomSheetLayout { [rootView] in
            rootView.bind(viewModel: viewModel)
        }
    }
}

extension ShareViewController: ModalSheetPresenterDelegate {
    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        rootView.contactsGridView.collectionView.contentOffset.y <= 0
    }
}

private extension ShareViewController {
    func wireHandlers() {
        rootView.didTapShare = { [weak self] in
            self?.presenter.didTapShare()
        }
        rootView.didTapCancel = { [weak self] in
            self?.presenter.didTapCancel()
        }
        rootView.didTapTrailingHeaderIcon = { [weak self] in
            self?.presenter.didTapSystemShare()
        }
    }
}
