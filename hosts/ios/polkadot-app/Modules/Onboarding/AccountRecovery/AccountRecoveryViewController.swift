import UIKit
import UIKit_iOS
import Foundation_iOS
import FoundationExt

final class AccountRecoveryViewController: UIViewController, ViewHolder {
    typealias RootViewType = AccountRecoveryViewLayout

    let presenter: AccountRecoveryPresenterProtocol
    private let screenCaptureProtector: ScreenCaptureProtector

    var keyboardHandler: KeyboardHandler?

    // MARK: - Lifecycle

    init(
        presenter: AccountRecoveryPresenterProtocol,
        screenCaptureProtector: ScreenCaptureProtector
    ) {
        self.presenter = presenter
        self.screenCaptureProtector = screenCaptureProtector
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = AccountRecoveryViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        addHandlers()
        protectInput()

        presenter.setup()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        if keyboardHandler == nil {
            setupKeyboardHandler()
        }
    }

    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)

        rootView.setInputCovered(screenCaptureProtector.isExposed)
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        rootView.textView.becomeFirstResponder()
    }
}

// MARK: - AccountRecoveryViewProtocol

extension AccountRecoveryViewController: AccountRecoveryViewProtocol {
    func didReceive(inputViewModel: any InputViewModelProtocol) {
        rootView.bind(inputViewModel: inputViewModel)
    }
}

// MARK: - KeyboardAdoptable

extension AccountRecoveryViewController: KeyboardAdoptable {}

// MARK: - Private

private extension AccountRecoveryViewController {
    func addHandlers() {
        rootView.proceedButton.addTarget(
            self,
            action: #selector(actionProceed),
            for: .touchUpInside
        )

        ThirdPartyKeyboardPolicy.restrict(rootView.textView)
    }

    @objc
    func actionProceed() {
        presenter.proceed()
    }

    func protectInput() {
        screenCaptureProtector.protect([rootView.textView], in: self) { [weak self] isExposed in
            self?.rootView.setInputCovered(isExposed)
        }
    }
}
