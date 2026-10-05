import AsyncExtensions
import Foundation
import BigInt
import ExtrinsicService
import SubstrateSdk
import Coinage
import UIKitExt
import ChainRegistry

final class TransferAmountPresenter {
    weak var view: TransferAmountViewProtocol?

    let interactor: TransferAmountInteractorInputProtocol
    let wireframe: TransferAmountWireframeProtocol

    let chainAsset: ChainAsset
    let config: TransferAmountConfig

    let dataValidationFactory: TransferDataValidatorFactoryProtocol
    let balanceViewModelFactory: BalanceViewModelFactoryProtocol
    let availableBalanceViewModelFactory: BalanceViewModelFactoryProtocol
    let amountInputStrategy: AmountInputStrategyProtocol
    let paymentAssetViewModelFactory: PaymentAssetViewModelMaking

    private var inputAmount: AmountInputResult?

    private var fee: ExtrinsicFeeProtocol? = ExtrinsicFee(amount: 0, payer: nil, weight: .zero)

    private var spendableBreakdown: TransferSpendableBreakdown?

    private var transferTask: Task<Void, Never>?
    private var statusTask: Task<Void, Never>?

    deinit {
        transferTask?.cancel()
        statusTask?.cancel()
    }

    init(
        interactor: TransferAmountInteractorInputProtocol,
        wireframe: TransferAmountWireframeProtocol,
        chainAsset: ChainAsset,
        balanceViewModelFactory: BalanceViewModelFactoryProtocol,
        availableBalanceViewModelFactory: BalanceViewModelFactoryProtocol,
        amountInputStrategy: AmountInputStrategyProtocol,
        dataValidationFactory: TransferDataValidatorFactoryProtocol,
        config: TransferAmountConfig = .default,
        paymentAssetViewModelFactory: PaymentAssetViewModelMaking = PaymentAssetViewModelFactory()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.chainAsset = chainAsset
        self.balanceViewModelFactory = balanceViewModelFactory
        self.availableBalanceViewModelFactory = availableBalanceViewModelFactory
        self.amountInputStrategy = amountInputStrategy
        self.dataValidationFactory = dataValidationFactory
        self.config = config
        self.paymentAssetViewModelFactory = paymentAssetViewModelFactory
    }
}

extension TransferAmountPresenter: TransferAmountPresenterProtocol {
    func setup() {
        applyConfig()

        interactor.setup()

        provideRecipientViewModel()
        provideAssetViewModel()
        view?.didReceive(paymentAsset: paymentAssetViewModelFactory.makeViewModel())
        provideAmountViewModel()
        provideAvailableBalance()
        provideFeeViewModel()
    }

    func confirm() {
        let amountDecimal = calculateInputAmount()
        let validations = createBaseValidations(for: amountDecimal)

        DataValidationRunner(validators: validations).runValidation { [weak self] in
            guard let amountDecimal else {
                return
            }
            self?.checkPrivacyAndSubmit(amount: amountDecimal)
        }
    }

    func onBalance() {
        guard !config.isAmountLocked else { return }

        inputAmount = .rate(1)
        provideInputAmount()
        provideAmountViewModel()
        refreshFee()

        #if TESTNET_FEATURE
            refreshStrategyPreview()
        #endif
    }

    func changeAmount(_ newValue: Decimal?) {
        guard !config.isAmountLocked else { return }

        inputAmount = newValue.map { .absolute($0) }
        provideInputAmount()

        refreshFee()

        #if TESTNET_FEATURE
            refreshStrategyPreview()
        #endif
    }
}

extension TransferAmountPresenter: TransferAmountInteractorOutputProtocol {
    func didReceive(spendableBreakdown: TransferSpendableBreakdown) {
        self.spendableBreakdown = spendableBreakdown
        provideAvailableBalance()
        provideAmountViewModel()
        provideInputAmount()
        refreshFee()

        #if TESTNET_FEATURE
            refreshStrategyPreview()
        #endif
    }

    func didReceive(error: TransferAmountInteractorError) {
        switch error {
        case .transactionFailed,
             .internalError:
            view?.didStopSubmission()
            applyBaseValidations()

        case .feeFailed:
            wireframe.presentFeeStatus(on: view, locale: nil) { [weak self] in
                self?.refreshFee()
            }
        }
    }
}

@MainActor
private extension TransferAmountPresenter {
    func provideAvailableBalance() {
        guard let breakdown = spendableBreakdown else {
            return
        }

        let amount = availableBalanceViewModelFactory.amountFromValue(breakdown.availablePrivate).value(for: .current)
        view?.didReceive(availableBalance: amount)
    }

    func calculateMax() -> BigUInt? {
        spendableBreakdown.map { $0.availablePrivate + $0.gainingPrivacy }
    }

    func calculateAvailablePrivate() -> BigUInt? {
        spendableBreakdown.map(\.availablePrivate)
    }

    func provideInputAmount() {
        let maxAmount = calculateAvailablePrivate()
        let amount = inputAmount?.absoluteValue(
            from: maxAmount?.decimal(assetInfo: chainAsset.assetDisplayInfo) ?? 0
        )
        let amountInPlank = amount?.toSubstrateAmount(precision: chainAsset.assetDisplayInfo.assetPrecision)
        guard let amountInPlank else {
            return
        }
        let formatted = balanceViewModelFactory.amountFromValue(amountInPlank).value(for: .current)
        view?.didReceive(input: formatted)
    }

    func provideAmountViewModel() {
        let amountViewModel = amountInputStrategy.createInputViewModelFactory(
            for: inputAmount,
            balance: calculateAvailablePrivate()
        )

        view?.didReceive(amountViewModel: amountViewModel)
    }

    func provideRecipientViewModel() {
        guard let address = interactor.recipient.address(in: chainAsset.chain) else {
            assertionFailure()
            return
        }
        let viewModel = TransferRecipientViewModel(username: interactor.recipient.username, address: address)
        view?.didReceive(recipient: viewModel)
    }

    func provideAssetViewModel() {
        let viewModel = amountInputStrategy.createAssetViewModel()
        view?.didReceive(assetViewModel: viewModel)
    }

    func provideFeeViewModel() {
        guard let fee else {
            view?.didReceive(feeViewModel: nil)
            return
        }

        let feeViewModel = balanceViewModelFactory.balanceFromPrice(
            fee.amount,
            priceData: nil
        )
        .value(for: .current)

        view?.didReceive(feeViewModel: feeViewModel)
    }

    func refreshFee() {
        provideFeeViewModel()
        applyBaseValidations()
    }

    func createBaseValidations(for amountDecimal: Decimal?) -> [DataValidating] {
        // Receiver-only validators read recipient.accountId, which is a per-
        // payment placeholder when recipientIsPlaceholder — skip to avoid noise.
        let receiverChecks: [DataValidating] = config.recipientIsPlaceholder ? [] : [
            dataValidationFactory.receiverDiffers(
                recepient: interactor.recipient.accountId,
                sender: interactor.senderAddress,
                locale: .current
            ),
            dataValidationFactory.accountIsNotSystem(
                for: interactor.recipient.accountId,
                locale: .current
            )
        ]

        return receiverChecks + [
            dataValidationFactory.hasAmount(
                balance: amountDecimal,
                locale: .current
            ),
            dataValidationFactory.feeIsCalculated(
                fee: fee,
                locale: .current
            ),
            dataValidationFactory.canSpendAmountInPlank(
                balance: calculateMax(),
                spendingAmount: amountDecimal,
                asset: chainAsset.assetDisplayInfo,
                locale: .current
            )
        ]
    }

    func applyBaseValidations() {
        let amountDecimal = calculateInputAmount()
        DataValidationRunner(validators: createBaseValidations(for: amountDecimal))
            .runValidation { [weak view] in
                view?.didStopLoading()
                view?.didReceiveValidation(result: .valid)
            }
    }

    func calculateInputAmount() -> Decimal? {
        guard let maxAmount = calculateAvailablePrivate() else {
            return nil
        }

        return inputAmount?.absoluteValue(
            from: maxAmount.decimal(assetInfo: chainAsset.assetDisplayInfo)
        )
    }

    func checkPrivacyAndSubmit(amount: Decimal) {
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                let validation = try await interactor.previewTransfer(for: amount)
                if validation.requiresPrivacyConfirmation {
                    presentPrivacyConfirmation(validation: validation)
                } else {
                    doSubmit(validation: validation)
                }
            } catch {
                view?.didStopSubmission()
                showTransferFailed(error)
            }
        }
    }

    /// The spend dips into gaining-privacy funds: confirm before submitting
    func presentPrivacyConfirmation(validation: TransferPreviewValidation) {
        let amountText = formattedAmount(validation.fullAmount)
        wireframe.showGainingPrivacyConfirmation(
            from: view,
            amount: amountText,
            onSendAnyway: { [weak self] in
                self?.doSubmit(validation: validation)
            },
            onCancel: { [weak self] in
                self?.view?.didStopSubmission()
            }
        )
    }

    func doSubmit(validation: TransferPreviewValidation) {
        view?.didStartSubmission()
        transferTask?.cancel()
        statusTask?.cancel()

        transferTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await interactor.confirmTransfer(validation: validation)
                if !config.recipientIsPlaceholder {
                    interactor.saveRecentContact()
                }

                observeLifecycle()
            } catch {
                failSubmission(error)
            }
        }
    }

    /// Drives completion from the lifecycle stream after a successful submission.
    /// The stream terminates after the last meaningful state; a graceful finish
    /// without a `.failed` status means the transfer succeeded. Flows without
    /// tracking finish immediately, completing right after submission.
    ///
    /// Runs in a separate task that holds `self` weakly per iteration, so a long
    /// claim wait never retains the presenter after the screen is dismissed.
    func observeLifecycle() {
        let statusStream = interactor.lifecycleStream()

        statusTask = Task { @MainActor [weak self] in
            var failed = false

            do {
                for try await state in statusStream {
                    if state.status == .failed {
                        failed = true
                    }
                    self?.apply(transferState: state)
                }
            } catch {
                failed = true
            }

            guard !Task.isCancelled else { return }
            guard !failed else {
                self?.failSubmission(TransferLifecycleError.transferFailed)
                return
            }
            self?.completeTransfer()
        }
    }

    func apply(transferState: OutgoingTransferState) {
        view?.didReceive(transferState: transferState)
        if transferState.status == .sent {
            view?.didUnlockNavigation()
        }
    }

    func failSubmission(_ error: Error) {
        view?.didStopSubmission()
        showTransferFailed(error)
    }

    func completeTransfer() {
        showTransferSuccess()
    }

    func formattedAmount(_ amount: BigUInt) -> String {
        balanceViewModelFactory.amountFromValue(amount)
            .value(for: .current)
    }

    func applyConfig() {
        guard let prefilledAmount = config.prefilledAmountInPlanks else { return }

        let decimal = prefilledAmount.decimal(assetInfo: chainAsset.assetDisplayInfo)
        inputAmount = .absolute(decimal)

        if config.isAmountLocked {
            view?.setAmountInputEnabled(false)
        }
    }

    func showTransferSuccess() {
        wireframe.hide(view: view)
    }

    func showTransferFailed(_ error: Error) {
        if !wireframe.present(error: error, from: view) {
            wireframe.presentTransactionFailure(from: view, onRetry: nil)
        }
    }
}

// MARK: - DEBUG

#if TESTNET_FEATURE
    extension TransferAmountPresenter {
        @MainActor
        func didReceive(strategyDebugInfo: TransferStrategyDebugInfo?) {
            view?.didReceive(strategyDebugInfo: strategyDebugInfo)
        }

        @MainActor
        func refreshStrategyPreview() {
            guard let amount = calculateInputAmount(), amount > 0 else {
                view?.didReceive(strategyDebugInfo: nil)
                return
            }
            interactor.previewStrategy(for: amount)
        }
    }
#endif
