import Foundation
import FoundationExt
import SubstrateSdk
import TrUAPIHost

// MARK: - Deposit

extension FundingViewModelFactory {
    func makeDepositViewModel(for state: FundingFlowState) -> FundingDepositViewModel {
        let title =
            switch state.rail {
            case .card: String(localized: .Funding.summaryCardTitle)
            case .bank: String(localized: .Funding.summaryBankTitle)
            case .crypto: String(localized: .Funding.summaryCryptoTitle)
            }
        let action: FundingDepositViewModel.Action =
            if state.failure != nil {
                .close
            } else if state.hasStarted {
                .cancel
            } else {
                .none
            }
        let isEnded = state.session.map { !$0.stage.isOpen } ?? false

        return FundingDepositViewModel(
            title: title,
            content: depositContent(for: state),
            action: action,
            paymentSeen: state.hasStarted && (isEnded || state.hasReached(.received))
        )
    }
}

private extension FundingViewModelFactory {
    func depositContent(for state: FundingFlowState) -> FundingDepositViewModel.Content {
        if let failure = state.failure {
            return .error(failure.failedText)
        }
        if let deposit = state.progress?.deposit {
            return depositDetails(deposit)
        }
        if let startError = state.startError {
            return .error(startError)
        }
        if state.noProviderQuoted, !state.hasStarted {
            return .error(state.quoteFailureText ?? String(localized: .Funding.errorNoProvider))
        }

        let pending: LocalizedStringResource =
            if !state.hasStarted {
                .Funding.depositFindingProvider
            } else {
                state.rail == .card ? .Funding.depositCardInProgress : .Funding.depositPreparing
            }
        return .pending(String(localized: pending))
    }

    func depositDetails(_ deposit: FundingDeposit) -> FundingDepositViewModel.Content {
        let amount = depositAmount(deposit)

        switch deposit {
        case let .crypto(address, network, _, _, _, exact, uri, _):
            return .crypto(FundingDepositViewModel.Crypto(
                payload: uri ?? address,
                amountTitle: String(localized: exact ? .Funding.depositExactAmount : .Funding.depositAtLeast),
                amount: amount,
                addressTitle: String(localized: .Funding.depositAddressOn(network: FundingNetwork(id: network).name)),
                address: address
            ))
        case let .bank(_, _, _, beneficiary, account, bankCode, reference, _):
            let lines = [
                amount.map { FundingCopyLineViewModel(title: String(localized: .Funding.depositExactAmount), text: $0) }
            ] + bankLines(beneficiary: beneficiary, account: account, bankCode: bankCode, reference: reference)
            return .bank(lines.compactMap(\.self))
        }
    }

    /// What to pay into the deposit the provider named.
    func depositAmount(_ deposit: FundingDeposit?) -> String? {
        switch deposit {
        case let .crypto(_, _, asset, amount, decimals, _, _, _):
            assetAmount(units: amount, asset: asset, decimals: decimals)
        case let .bank(amount, currency, decimals, _, _, _, _, _):
            assetAmount(units: amount, asset: currency, decimals: decimals)
        case nil:
            nil
        }
    }

    func bankLines(
        beneficiary: String?,
        account: String?,
        bankCode: String?,
        reference: String
    ) -> [FundingCopyLineViewModel?] {
        [
            beneficiary
                .map { FundingCopyLineViewModel(title: String(localized: .Funding.depositBeneficiary), text: $0) },
            account.map { FundingCopyLineViewModel(title: String(localized: .Funding.depositAccount), text: $0) },
            bankCode.map { FundingCopyLineViewModel(title: String(localized: .Funding.depositBankCode), text: $0) },
            FundingCopyLineViewModel(title: String(localized: .Funding.depositReference), text: reference)
        ]
    }
}

// MARK: - Progress

extension FundingViewModelFactory {
    func makeProgressViewModel(for state: FundingFlowState) -> FundingProgressViewModel {
        let quote = state.chosenQuote
        let cashAmount = state.session
            .flatMap { FundingActivityItem(session: $0, progress: state.progress).amount }
            ?? quote.flatMap { Balance(units: state.direction == .in ? $0.receiveAmount : $0.sendAmount) }
        let progress = state.progress
        let isSlow = progress.map { [.retrying, .delayed].contains($0.band) } ?? false

        return FundingProgressViewModel(
            figure: cashAmount.map(cashFigure),
            symbol: cashSymbol,
            isSlow: isSlow,
            steps: progressSteps(for: state, isSlow: isSlow),
            stepsBetween: FundingStepBar.between(progress?.band),
            note: progressNote(for: state, amount: cashAmount.map(cashLabel)),
            liveLine: progress.flatMap { progress in
                guard progress.lastHeardAtMs != nil || progress.detail != nil else { return nil }
                return FundingProgressViewModel.LiveLine(
                    heardAt: progress.lastHeardAtMs.map(Date.init(milliseconds:)),
                    detail: progress.detail
                )
            },
            details: progressDetails(for: state),
            hasFees: quote != nil,
            arrival: quote?.etaSecs.map { eta in
                FundingProgressViewModel.Arrival(
                    eta: FundingEta.text(seconds: eta),
                    rateNote: state.direction == .in && state.rail != .card ? bankRateNote : nil
                )
            },
            cancelTitle: state.canRequestCancel ? state.cancelTitle : nil,
            cancelOffer: state.cancelOffer.map { offer in
                FundingProgressViewModel.CancelOffer(
                    title: String(localized: state.direction == .out ? .Funding.offerTitleWithdraw : .Funding
                        .offerTitle),
                    fee: cashLabel(offer.fee),
                    refund: cashLabel(offer.refund),
                    terms: CancelTerms(fee: offer.fee.units, refund: offer.refund.units)
                )
            },
            isAnsweringOffer: state.isAnsweringOffer,
            hasStarted: state.hasStarted,
            longerAtMs: progress?.longerAtMs,
            ended: state.ended
                .map { FundingProgressViewModel.Ended(session: $0, viewModel: makeEndedViewModel(for: $0)) }
        )
    }
}

private extension FundingViewModelFactory {
    func progressSteps(for state: FundingFlowState, isSlow: Bool) -> [FundingStepBar.Step] {
        let reported = state.progress?.steps ?? []
        let firstUnreached = reported.firstIndex { $0.reachedAtMs == nil }

        return reported.enumerated().map { index, step in
            let stepState: FundingStepBar.State =
                if step.reachedAtMs != nil {
                    .done
                } else if index == firstUnreached {
                    .current(isAmber: isSlow)
                } else {
                    .upcoming
                }
            return FundingStepBar.Step(
                title: FundingStepBar.title(step.step, direction: state.direction),
                state: stepState
            )
        }
    }

    /// Where a cancel stands, else the band's line, worded for the session's
    /// rail and amount.
    func progressNote(for state: FundingFlowState, amount: String?) -> FundingProgressViewModel.Note? {
        guard let progress = state.progress else { return nil }

        if let line = progress.status.cancelLine(direction: state.direction) {
            return FundingProgressViewModel.Note(text: line.text, isWarning: line.isWarning)
        }
        if progress.status.cancelCameTooLate {
            return FundingProgressViewModel.Note(text: String(localized: .Funding.cancelCameTooLate), isWarning: true)
        }
        let band = FundingWording.band(
            progress.band,
            direction: state.direction,
            rail: state.rail,
            asset: state.asset,
            amount: amount
        )
        return band.map { FundingProgressViewModel.Note(text: $0.text, isWarning: $0.isWarning) }
    }

    func progressDetails(for state: FundingFlowState) -> FundingProgressViewModel.Details {
        let quote = state.chosenQuote

        switch (state.direction, state.rail) {
        case (.in, .card):
            return .amount(
                title: String(localized: .Funding.progressYouSent),
                amount: quote.flatMap { quoteAmount($0.sendAmount, of: $0, in: state) }
            )
        case (.in, .bank):
            return .bank(progressBank(for: state))
        case (.in, .crypto):
            return .crypto(progressCryptoDeposit(for: state))
        case (.out, _):
            return .amount(
                title: String(localized: .Funding.summaryWithdrawCaption),
                amount: quote.flatMap { quoteAmount($0.receiveAmount, of: $0, in: state) }
            )
        }
    }

    /// "Send this exact amount" until the transfer is seen, then what was
    /// sent, with the account to pay into once the provider named it.
    func progressBank(for state: FundingFlowState) -> FundingProgressViewModel.Bank {
        let isReceived = state.hasReached(.received)
        let amountTitle = isReceived
            ? String(localized: .Funding.progressYouSent)
            : String(localized: .Funding.summaryBankCaption)

        if case let .bank(_, _, _, beneficiary, account, bankCode, reference, _)? = state.progress?.deposit {
            let lines = bankLines(beneficiary: beneficiary, account: account, bankCode: bankCode, reference: reference)
            return FundingProgressViewModel.Bank(
                amountTitle: amountTitle,
                amount: depositAmount(state.progress?.deposit),
                copyLines: lines.compactMap(\.self),
                isPreparing: false
            )
        }

        let reference = state.progress?.reference
        return FundingProgressViewModel.Bank(
            amountTitle: amountTitle,
            amount: state.chosenQuote.flatMap { quoteAmount($0.sendAmount, of: $0, in: state) },
            copyLines: reference.map {
                [FundingCopyLineViewModel(title: String(localized: .Funding.depositReference), text: $0)]
            } ?? [],
            isPreparing: reference == nil && !isReceived
        )
    }

    /// While the funds are not seen, the way back to the address to pay.
    func progressCryptoDeposit(for state: FundingFlowState) -> FundingProgressViewModel.CryptoDeposit {
        guard !state.hasReached(.received) else { return .hidden }
        guard case let .crypto(_, _, _, _, _, exact, _, _)? = state.progress?.deposit else { return .preparing }

        return .deposit(
            title: String(localized: exact ? .Funding.depositExactAmount : .Funding.depositAtLeast),
            amount: depositAmount(state.progress?.deposit)
        )
    }
}
