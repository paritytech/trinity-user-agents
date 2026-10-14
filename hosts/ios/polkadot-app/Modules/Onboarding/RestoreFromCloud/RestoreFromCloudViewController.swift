import UIKit
import FoundationExt

final class RestoreFromCloudViewController: UIViewController, ViewHolder {
    typealias RootViewType = RestoreFromCloudViewLayout

    let presenter: RestoreFromCloudPresenterProtocol

    init(presenter: RestoreFromCloudPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RestoreFromCloudViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        presenter.viewDidAppear()
    }
}

extension RestoreFromCloudViewController: RestoreFromCloudViewProtocol {
    func didReceive(viewModel: RestoreFromCloudViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}
