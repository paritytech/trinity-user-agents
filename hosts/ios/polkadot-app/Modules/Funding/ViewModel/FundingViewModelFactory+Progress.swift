import Foundation
import FoundationExt
import SubstrateSdk
import TrUAPIHost

// MARK: - Deposit

extension FundingViewModelFactory {
    func makeDepositViewModel(for model: FundingEntryModel) -> FundingDepositViewModel {
        let title =
            switch model.rail {
            case .card: String(localized: .Funding.summaryCardTitle)
            case .bank: String(localized: .Funding.summaryBankTitle)
            case .crypto: String(localized: .Funding.summaryCryptoTitle)
            }
        let action: FundingDepositViewModel.Action =
            if model.failure != nil {
                .close
            } else if model.hasStarted {
                .cancel
            } else {
                .none
            }
        let isEnded = model.session.map { !$0.stage.isOpen } ?? false

        return FundingDepositViewModel(
            title: title,
            content: depositContent(for: model),
            action: action,
            paymentSeen: model.hasStarted && (isEnded || model.hasReached(.received))
        )
    }
}

private extension FundingViewModelFactory {
    func depositContent(for model: FundingEntryModel) -> FundingDepositViewModel.Content {
        if let failure = model.failure {
            return .error(failure.failedText)
        }
        if let deposit = model.progress?.deposit {
            return depositDetails(deposit)
        }
        if let startError = model.startError {
            return .error(startError)
        }
        if model.noProviderQuoted, !model.hasStarted {
            return .error(model.quoteFailureText ?? String(localized: .Funding.errorNoProvider))
        }

        let pending: LocalizedStringResource =
            if !model.hasStarted {
                .Funding.depositFindingProvider
            } else {
                model.rail == .card ? .Funding.depositCardInProgress : .Funding.depositPreparing
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
    func makeProgressViewModel(for model: FundingEntryModel) -> FundingProgressViewModel {
        let quote = model.chosenQuote
        let cashAmount = model.session
            .flatMap { FundingActivityItem(session: $0, progress: model.progress).amount }
            ?? quote.flatMap { Balance(units: model.direction == .in ? $0.receiveAmount : $0.sendAmount) }
        let progress = model.progress
        let isSlow = progress.map { [.retrying, .delayed].contains($0.band) } ?? false

        return FundingProgressViewModel(
            figure: cashAmount.map(cashFigure),
            symbol: cashSymbol,
            isSlow: isSlow,
            steps: progressSteps(for: model, isSlow: isSlow),
            stepsBetween: FundingStepBar.between(progress?.band),
            note: progressNote(for: model, amount: cashAmount.map(cashLabel)),
            liveLine: progress.flatMap { progress in
                guard progress.lastHeardAtMs != nil || progress.detail != nil else { return nil }
                return FundingProgressViewModel.LiveLine(
                    heardAt: progress.lastHeardAtMs.map(Date.init(milliseconds:)),
                    detail: progress.detail
                )
            },
            details: progressDetails(for: model),
            hasFees: quote != nil,
            arrival: quote?.etaSecs.flatMap(etaText(seconds:)).map { eta in
                FundingProgressViewModel.Arrival(
                    eta: eta,
                    rateNote: model.direction == .in && model.rail != .card ? bankRateNote : nil
                )
            },
            cancelTitle: model.canRequestCancel ? model.cancelTitle : nil,
            cancelOffer: model.cancelOffer.map { offer in
                FundingProgressViewModel.CancelOffer(
                    title: String(localized: model.direction == .out ? .Funding.offerTitleWithdraw : .Funding
                        .offerTitle),
                    fee: cashLabel(offer.fee),
                    refund: cashLabel(offer.refund),
                    terms: CancelTerms(fee: offer.fee.units, refund: offer.refund.units)
                )
            },
            isAnsweringOffer: model.isAnsweringOffer,
            ended: model.ended
                .map { FundingProgressViewModel.Ended(session: $0, viewModel: makeEndedViewModel(for: $0)) }
        )
    }
}

private extension FundingViewModelFactory {
    func progressSteps(for model: FundingEntryModel, isSlow: Bool) -> [FundingStepBar.Step] {
        let reported = model.progress?.steps ?? []
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
                title: FundingStepBar.title(step.step, direction: model.direction),
                state: stepState
            )
        }
    }

    /// Where a cancel stands, else the band's line, worded for the session's
    /// rail and amount.
    func progressNote(for model: FundingEntryModel, amount: String?) -> FundingProgressViewModel.Note? {
        guard let progress = model.progress else { return nil }

        if let line = progress.status.cancelLine(direction: model.direction) {
            return FundingProgressViewModel.Note(text: line.text, isWarning: line.isWarning)
        }
        if progress.status.cancelCameTooLate {
            return FundingProgressViewModel.Note(text: String(localized: .Funding.cancelCameTooLate), isWarning: true)
        }
        let band = FundingWording.band(
            progress.band,
            direction: model.direction,
            rail: model.rail,
            asset: model.asset,
            amount: amount
        )
        return band.map { FundingProgressViewModel.Note(text: $0.text, isWarning: $0.isWarning) }
    }

    func progressDetails(for model: FundingEntryModel) -> FundingProgressViewModel.Details {
        let quote = model.chosenQuote

        switch (model.direction, model.rail) {
        case (.in, .card):
            return .amount(
                title: String(localized: .Funding.progressYouSent),
                amount: quote.flatMap { quoteAmount($0.sendAmount, of: $0, in: model) }
            )
        case (.in, .bank):
            return .bank(progressBank(for: model))
        case (.in, .crypto):
            return .crypto(progressCryptoDeposit(for: model))
        case (.out, _):
            return .amount(
                title: String(localized: .Funding.summaryWithdrawCaption),
                amount: quote.flatMap { quoteAmount($0.receiveAmount, of: $0, in: model) }
            )
        }
    }

    /// "Send this exact amount" until the transfer is seen, then what was
    /// sent, with the account to pay into once the provider named it.
    func progressBank(for model: FundingEntryModel) -> FundingProgressViewModel.Bank {
        let isReceived = model.hasReached(.received)
        let amountTitle = isReceived
            ? String(localized: .Funding.progressYouSent)
            : String(localized: .Funding.summaryBankCaption)

        if case let .bank(_, _, _, beneficiary, account, bankCode, reference, _)? = model.progress?.deposit {
            let lines = bankLines(beneficiary: beneficiary, account: account, bankCode: bankCode, reference: reference)
            return FundingProgressViewModel.Bank(
                amountTitle: amountTitle,
                amount: depositAmount(model.progress?.deposit),
                copyLines: lines.compactMap(\.self),
                isPreparing: false
            )
        }

        let reference = model.progress?.reference
        return FundingProgressViewModel.Bank(
            amountTitle: amountTitle,
            amount: model.chosenQuote.flatMap { quoteAmount($0.sendAmount, of: $0, in: model) },
            copyLines: reference.map {
                [FundingCopyLineViewModel(title: String(localized: .Funding.depositReference), text: $0)]
            } ?? [],
            isPreparing: reference == nil && !isReceived
        )
    }

    /// While the funds are not seen, the way back to the address to pay.
    func progressCryptoDeposit(for model: FundingEntryModel) -> FundingProgressViewModel.CryptoDeposit {
        guard !model.hasReached(.received) else { return .hidden }
        guard case let .crypto(_, _, _, _, _, exact, _, _)? = model.progress?.deposit else { return .preparing }

        return .deposit(
            title: String(localized: exact ? .Funding.depositExactAmount : .Funding.depositAtLeast),
            amount: depositAmount(model.progress?.deposit)
        )
    }
}
