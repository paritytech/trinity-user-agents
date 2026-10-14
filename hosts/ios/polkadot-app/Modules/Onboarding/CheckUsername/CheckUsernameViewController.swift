import UIKit
import FoundationExt

final class CheckUsernameViewController: UIViewController, ViewHolder {
    typealias RootViewType = CheckUsernameViewLayout

    let presenter: CheckUsernamePresenterProtocol

    init(presenter: CheckUsernamePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = CheckUsernameViewLayout()
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

extension CheckUsernameViewController: CheckUsernameViewProtocol {
    func didReceive(viewModel: CheckUsernameViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}
