import UIKit

final class LocalAuthTransparentViewController: UIViewController {
    let presenter: LocalAuthPresenterProtocol

    init(presenter: LocalAuthPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
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

extension LocalAuthTransparentViewController: LocalAuthViewProtocol {
    func didStartAuth() {}

    func didStopAuth() {}
}
