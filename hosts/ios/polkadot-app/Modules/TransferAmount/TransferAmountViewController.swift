import UIKit
import UIKit_iOS
import Foundation_iOS
import FoundationExt
import PolkadotUI

final class TransferAmountViewController: UIViewController, ViewHolder {
    typealias RootViewType = TransferAmountViewLayout

    let presenter: TransferAmountPresenterProtocol

    var keyboardHandler: KeyboardHandler?
    private var isAmountInputEnabled = true
    private var paymentAssetViewModel: PaymentAssetViewModelProtocol?

    init(presenter: TransferAmountPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    deinit {
        paymentAssetViewModel?.cancel()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RootViewType()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupLocalization()
        setupHandlers()

        // Fee view is hidden
        rootView.feeView.isHidden = true

        presenter.setup()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        if keyboardHandler == nil {
            setupKeyboardHandler()
        }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        if isAmountInputEnabled {
            rootView.amountInputView.textField.becomeFirstResponder()
        }
    }

    func setupHandlers() {
        rootView.amountInputView.addTarget(
            self,
            action: #selector(actionAmountChanged),
            for: .editingChanged
        )

        rootView.onAction = { [weak self] in
            self?.actionConfirm()
        }

        let maxAction = UIAction { [weak presenter] _ in
            presenter?.onBalance()
        }
        rootView.balanceView.addAction(maxAction, for: .touchUpInside)
    }

    func lockScreenNavigation() {
        navigationItem.leftBarButtonItem?.isHidden = true
        navigationItem.hidesBackButton = true
        navigationController?.interactivePopGestureRecognizer?.isEnabled = false
    }

    func unlockScreenNavigation() {
        navigationItem.leftBarButtonItem?.isHidden = false
        navigationItem.hidesBackButton = false
        navigationController?.interactivePopGestureRecognizer?.isEnabled = true
    }

    func setupLocalization() {
        title = String(localized: .transferMainTitle)
        rootView.balanceView.titleLabel.text = String(localized: .transferBalanceViewTitle)
    }

    @objc func actionAmountChanged() {
        let decimalAmount = rootView.amountInputView.inputViewModel?.decimalAmount
        presenter.changeAmount(decimalAmount)
    }

    func actionConfirm() {
        rootView.amountInputView.textField.resignFirstResponder()
        presenter.confirm()
    }

    func handle(validation result: ValidationResult) {
        rootView.confirmView.isUserInteractionEnabled = true

        let confirmState: TransferConfirmState
        let isUserInteractionEnabled: Bool

        switch result {
        case let .issue(_, context) where context is CommonValidationIssueContext:
            // swiftlint:disable:next force_cast
            let commonContext = context as! CommonValidationIssueContext

            let info = commonContext.additionalInfo
            rootView.issueLabel.text = info
            rootView.issueLabel.isHidden = (info == nil)

            switch commonContext {
            case .insufficientBalance:
                confirmState = .issue(String(localized: .transferActionJustSend))
                isUserInteractionEnabled = true
            case .calculatingFee:
                isUserInteractionEnabled = false
                confirmState = .loading(status: nil)
            }
        case let .issue(title, _):
            confirmState = .issue(title)
            isUserInteractionEnabled = true
            rootView.issueLabel.isHidden = true
        case .valid:
            confirmState = .confirm
            isUserInteractionEnabled = true
            rootView.issueLabel.isHidden = true
        }

        rootView.bind(state: confirmState)
        rootView.confirmView.isUserInteractionEnabled = isUserInteractionEnabled
    }
}

extension TransferAmountViewController: KeyboardAdoptable {
    func updateWhileKeyboardFrameChanging(_ frame: CGRect) {
        let localKeyboardFrame = view.convert(frame, from: nil)
        let bottomInset = view.bounds.height - localKeyboardFrame.minY

        if bottomInset > 0 {
            rootView.adoptToVisibleKeyboard(bottomInset: bottomInset)
        } else {
            rootView.adoptToHiddenKeyboard()
        }

        rootView.layoutIfNeeded()
    }
}

private extension OutgoingTransferState.Status {
    var sendingStatusTitle: String {
        switch self {
        case .sending:
            String(localized: .transferStatusSending)
        case .sent:
            String(localized: .transferStatusSent)
        case .claimed:
            String(localized: .transferStatusFinished)
        case .failed:
            String(localized: .transferStatusError)
        }
    }
}

extension TransferAmountViewController: TransferAmountViewProtocol {
    func setAmountInputEnabled(_ enabled: Bool) {
        isAmountInputEnabled = enabled

        rootView.balanceView.isUserInteractionEnabled = enabled
        rootView.amountInputView.textField.isEnabled = enabled
    }

    func didReceive(input: String) {
        rootView.setActionAmount(input)
    }

    func didReceive(recipient viewModel: TransferRecipientViewModel) {
        rootView.bind(recipient: viewModel)
    }

    func didReceive(availableBalance: String) {
        rootView.bind(availableBalance: availableBalance)
    }

    func didReceive(amountViewModel: AmountInputViewModelProtocol) {
        rootView.amountInputView.bind(inputViewModel: amountViewModel)
    }

    func didReceive(assetViewModel: AssetAmountViewModel) {
        rootView.amountInputView.bind(assetViewModel: assetViewModel)
    }

    func didReceive(paymentAsset: PaymentAssetViewModelProtocol) {
        paymentAssetViewModel?.cancel()
        paymentAssetViewModel = paymentAsset

        paymentAsset.bind { [weak self] brand in
            self?.rootView.apply(assetBrand: brand)
        }
    }

    func didReceive(feeViewModel _: BalanceViewModelProtocol?) {
        // Do not handle fee
    }

    func didReceiveValidation(result: ValidationResult) {
        guard !rootView.isLoading else { return }
        handle(validation: result)
    }

    func didStartSubmission() {
        lockScreenNavigation()
        didStartLoading()
    }

    func didStopSubmission() {
        didStopLoading()
        unlockScreenNavigation()
    }

    func didStartLoading() {
        rootView.bind(state: .loading(status: nil))
    }

    func didStopLoading() {
        rootView.bind(state: .confirm)
    }

    func didReceive(transferState: OutgoingTransferState) {
        rootView.bind(transferStatus: transferState.status.sendingStatusTitle)
    }

    func didUnlockNavigation() {
        unlockScreenNavigation()
    }

    #if TESTNET_FEATURE
        func didReceive(strategyDebugInfo: TransferStrategyDebugInfo?) {
            rootView.bind(strategyDebugInfo: strategyDebugInfo)
        }
    #endif
}
