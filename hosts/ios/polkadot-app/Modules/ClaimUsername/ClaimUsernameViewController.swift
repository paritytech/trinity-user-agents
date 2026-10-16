import UIKit
import SwiftUI
import DesignSystem
import Foundation_iOS
import FoundationExt
import PolkadotUI

struct ClaimUsernameContentViewModel {
    let headerText: String
    let title: String
    let details: String
    let actionTitle: String
    let recoveryActionString: NSAttributedString?
    let termsActionString: AttributedString?
}

class ClaimUsernameViewController: UIHostingController<ClaimUsernameViewLayout> {
    let presenter: ClaimUsernamePresenterProtocol

    init(presenter: ClaimUsernamePresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: ClaimUsernameViewLayout(viewModel: .init()))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .backgroundPrimary
        setupHandlers()
        presenter.setup()
    }

    private func setupHandlers() {
        rootView.viewModel.onUsernameChanged = { [weak self, weak presenter] in
            guard let self, let inputViewModel = rootView.viewModel.usernameInputViewModel else { return }
            presenter?.update(from: inputViewModel)
        }
        rootView.viewModel.onDigitsChanged = { [weak self, weak presenter] in
            guard let self else { return }
            guard let selected = rootView.viewModel.selectedDigits else { return }
            presenter?.updateDigits(selected)
        }
        rootView.viewModel.onConfirm = { [weak presenter] in
            presenter?.confirm()
        }
        rootView.viewModel.onResolveError = { [weak presenter] in
            presenter?.resolveError()
        }
        rootView.viewModel.onRecover = { [weak presenter] in
            presenter?.recover()
        }
    }
}

extension ClaimUsernameViewController: ClaimUsernameViewProtocol {
    func didReceive(viewModel: ClaimUsernameContentViewModel) {
        rootView.viewModel.headerText = viewModel.headerText
        rootView.viewModel.title = viewModel.title
        rootView.viewModel.details = viewModel.details
        rootView.viewModel.actionTitle = viewModel.actionTitle
        if let nsAttr = viewModel.recoveryActionString {
            rootView.viewModel.recoveryActionString = try? AttributedString(nsAttr, including: \.uiKit)
        } else {
            rootView.viewModel.recoveryActionString = nil
        }
        rootView.viewModel.termsActionString = viewModel.termsActionString
    }

    func didReceive(usernameInputViewModel: InputViewModelProtocol) {
        rootView.viewModel.usernameInputViewModel = usernameInputViewModel
    }

    func didReceive(digitsOptions: [String]) {
        rootView.viewModel.digitsOptions = digitsOptions
        rootView.viewModel.selectedDigits = digitsOptions.first
    }

    func didReceive(digitsState: DigitsFieldState) {
        rootView.viewModel.digitsState = digitsState
    }

    func didStartLoading() {
        rootView.viewModel.confirmViewState = .loading
    }

    func didStopLoading() {
        rootView.viewModel.confirmViewState = .confirm
    }

    func didReceiveValidation(result: ValidationResult) {
        guard !isLoading else { return }
        switch result {
        case let .issue(title, context):
            rootView.viewModel.confirmViewState = .issue(title)
            apply(usernameContext: context as? UsernameValidationContext)
        case .valid:
            rootView.viewModel.usernameAvailability = .available
            rootView.viewModel.confirmViewState = .confirm
        }
    }

    func userInteraction(enabled: Bool) {
        rootView.viewModel.isUsernameInteractionEnabled = enabled
    }

    func setAccountCreationInProgress(_ inProgress: Bool) {
        rootView.viewModel.isAccountCreationInProgress = inProgress
    }

    private var isLoading: Bool {
        if case .loading = rootView.viewModel.confirmViewState {
            return true
        }
        return false
    }

    private func apply(usernameContext: UsernameValidationContext?) {
        switch usernameContext {
        case .usernameTaken:
            rootView.viewModel.usernameAvailability = .taken
            rootView.viewModel.confirmViewState = .errorAction(String(localized: .Common.clear), nil)
        case .usernameInvalid:
            rootView.viewModel.usernameAvailability = .invalid
            rootView.viewModel.confirmViewState = .errorAction(String(localized: .Common.clear), nil)
        case .usernameCheckFailed:
            rootView.viewModel.usernameAvailability = nil
            rootView.viewModel.confirmViewState = .errorAction(String(localized: .Common.retry), nil)
        case nil:
            rootView.viewModel.usernameAvailability = nil
        }
    }
}

final class ClaimLiteUsernameViewController: ClaimUsernameViewController {}
extension ClaimLiteUsernameViewController: HiddableBarWhenPushed {}
