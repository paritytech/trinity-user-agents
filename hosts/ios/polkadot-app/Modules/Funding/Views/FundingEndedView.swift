import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// An ended session's detail screen: what moved, how, through whom, the ids
/// to quote to support, and when. A failure also shows the step it stopped
/// on and why, with a way to start a new session.
struct FundingEndedView: View {
    let ended: FundingEndedSession
    let cash: FundingCash
    let branding: FundingProviderBranding
    var onFees: (() -> Void)?
    let onClose: () -> Void
    let onStartOver: () -> Void

    @State private var copied: String?
    @State private var providerName: String?

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: "", onBack: onClose)

            ScrollView {
                VStack(spacing: DSSpacings.medium) {
                    headline
                    if !ended.succeeded {
                        stepCard
                    }
                    rows
                }
            }

            bottomAction
        }
        .fundingToast($copied)
        .task(id: ended.record.providerId) { await loadProviderName() }
    }
}

// MARK: - Sections

private extension FundingEndedView {
    var headline: some View {
        VStack(spacing: DSSpacings.small) {
            statusIcon
            if let units = ended.cashAmount {
                let value = cash.decimal(units)
                let figure = ended.succeeded ? signedFigure(value) : cash.figure(value)
                DSAmount(
                    amount: figure,
                    symbol: cash.symbol,
                    typography: .fundingFigure(digits: figure.filter(\.isNumber).count)
                )
                .foregroundStyle(amountColor)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
            }
            if ended.succeeded {
                Text(verbatim: FundingActivityDate.moment(ended.record.settledAt))
                    .typography(.bodySmall)
                    .foregroundStyle(.fgSecondary)
            }
        }
        .padding(.top, DSSpacings.small)
    }

    var statusIcon: some View {
        let name =
            if !ended.succeeded {
                "xmark"
            } else {
                ended.direction == .in ? "plus" : "arrow.up.right"
            }
        return Image(systemName: name)
            .font(.system(size: 16, weight: .semibold))
            .foregroundStyle(ended.succeeded ? Color.fgPrimary : Color.fgError)
            .frame(width: 40, height: 40)
            .background(ended.succeeded ? Color.bgSurfaceMain : Color.bgStatusError.opacity(0.25), in: Circle())
    }

    var amountColor: Color {
        guard ended.succeeded else { return .fgTertiary }
        return ended.direction == .in ? .fgSuccess : .fgPrimary
    }

    var stepCard: some View {
        VStack(spacing: 0) {
            FundingStepBar(steps: steps)
                .padding(.horizontal, DSSpacings.mediumIncreased)
                .padding(.vertical, DSSpacings.medium)
                .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))

            Text(verbatim: note)
                .typography(.bodySmall)
                .foregroundStyle(.fgSecondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, DSSpacings.medium)
                .padding(.vertical, DSSpacings.smallIncreased)
        }
        .background(.bgSurfaceNested, in: RoundedRectangle(cornerRadius: DSRadii.large))
    }

    /// What is known about the session, one row each; a value the record
    /// does not hold has no row.
    var rows: some View {
        VStack(spacing: DSSpacings.extraSmall) {
            if let paid = ended.quotedAmount {
                FundingValueRow(title: amountCaption, action: ended.quote == nil ? nil : onFees) {
                    value(paid)
                }
            }

            if let settled = ended.record.settledAmount {
                FundingValueRow(title: settledCaption) {
                    VStack(alignment: .trailing, spacing: DSSpacings.extraSmall) {
                        value(cash.label(cash.decimal(settled)))
                        if let requested = ended.differingRequestedAmount {
                            let amount = cash.label(cash.decimal(requested))
                            Text(verbatim: String(localized: .Funding.endedRequested(amount: amount)))
                                .typography(.bodySmall)
                                .foregroundStyle(.fgSecondary)
                                .multilineTextAlignment(.trailing)
                        }
                    }
                }
            }

            if let method {
                FundingValueRow(title: String(localized: .Funding.endedMethod)) {
                    value(method)
                }
            }

            if let providerName {
                FundingValueRow(title: String(localized: .Funding.summaryProvider)) {
                    value(providerName)
                }
            }

            if let payout = payoutText {
                FundingValueRow(title: String(localized: .Funding.endedPayout)) {
                    value(payout)
                }
            }

            if ended.payoutState == .pending, let eta = ended.quote?.etaSecs {
                FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                    value(FundingEta.text(seconds: eta))
                }
            }

            if let transactionId = nonEmpty(ended.record.transactionId) {
                FundingCopyRow(
                    title: String(localized: .Funding.endedTransactionId),
                    text: transactionId,
                    copied: $copied,
                    truncatesMiddle: true
                )
            }

            if let reference = nonEmpty(ended.record.reference) {
                FundingCopyRow(
                    title: String(localized: .Funding.depositReference),
                    text: reference,
                    copied: $copied
                )
            }

            FundingValueRow(title: String(localized: .Funding.endedStarted)) {
                value(dateTime(ended.record.openedAt))
            }

            FundingValueRow(title: endedCaption) {
                value(dateTime(ended.record.settledAt))
            }
        }
    }

    var bottomAction: some View {
        Group {
            if ended.succeeded {
                FundingPrimaryButton(
                    title: String(localized: .Funding.failureClose),
                    style: .secondary,
                    action: onClose
                )
            } else {
                FundingPrimaryButton(
                    title: String(localized: .Funding.endedStartOver),
                    style: .primary,
                    action: onStartOver
                )
            }
        }
    }
}

// MARK: - Text

private extension FundingEndedView {
    var steps: [FundingStepBar.Step] {
        let stoppedAt = ended.stoppedAt ?? ended.steps.count
        return ended.steps.enumerated().map { index, step in
            if index < stoppedAt {
                FundingStepBar.Step(title: FundingStepBar.title(step), state: .done)
            } else if index == stoppedAt {
                FundingStepBar.Step(title: stoppedTitle(step), state: .failed)
            } else {
                FundingStepBar.Step(title: FundingStepBar.title(step), state: .upcoming)
            }
        }
    }

    /// The failed step, named for what went wrong on it.
    func stoppedTitle(_ step: FundingStep) -> String {
        switch ended.outcome {
        case .succeeded:
            return FundingStepBar.title(step)
        case .payoutFailed:
            return String(localized: .Funding.activityPayoutFailed)
        case .failed(.refunded):
            return String(localized: .Funding.activityRefunded)
        case .failed:
            guard step == .payment else { return FundingStepBar.title(step) }
            if ended.direction == .out { return String(localized: .Funding.endedStepSendingFailed) }
            if ended.isDeclined, ended.record.rail == .bank {
                return String(localized: .Funding.endedStepPaymentDeclined)
            }
            return String(localized: .Funding.endedStepPaymentFailed)
        }
    }

    /// Why it did not go through, under the steps.
    var note: String {
        switch ended.outcome {
        case .succeeded:
            return ""
        case let .payoutFailed(reason):
            if ended.record.rail == .crypto { return String(localized: .Funding.progressPayoutFailed(reason: reason)) }
            return String(localized: .Funding.endedPayoutFailed)
        case let .failed(failure):
            return ended.direction == .in ? topUpNote(failure) : withdrawNote(failure)
        }
    }

    func topUpNote(_ failure: FundingFailure) -> String {
        if ended.isDeclined {
            switch ended.record.rail {
            case .bank: return String(localized: .Funding.endedBankDeclined)
            case .card: return String(localized: .Funding.endedCardDeclined)
            case .crypto,
                 nil: return String(localized: .Funding.endedTopUpFailed)
            }
        }

        switch failure {
        case .refunded:
            return refundedNote
        case .cancelled,
             .expired,
             .other:
            return String(localized: .Funding.endedTopUpFailed)
        default:
            return failure.reasonText
        }
    }

    var refundedNote: String {
        let paid = ended.quotedAmount
        switch ended.record.rail {
        case .bank:
            return paid.map { String(localized: .Funding.endedRefundedBank(amount: $0)) }
                ?? String(localized: .Funding.endedRefundedBankUnknown)
        case .crypto:
            return String(localized: .Funding.endedRefundedCrypto)
        case .card,
             nil:
            return paid.map { String(localized: .Funding.endedRefundedCard(amount: $0)) }
                ?? String(localized: .Funding.endedRefundedCardUnknown)
        }
    }

    func withdrawNote(_ failure: FundingFailure) -> String {
        let amount = ended.cashAmount.map { cash.label(cash.decimal($0)) } ?? "$\(cash.symbol)"
        switch failure {
        case .refunded:
            return String(localized: .Funding.endedWithdrawRefunded(amount: amount))
        case .cancelled,
             .expired,
             .other:
            return String(localized: .Funding.endedWithdrawFailed(amount: amount))
        default:
            return failure.reasonText
        }
    }

    var amountCaption: String {
        switch (ended.direction, ended.record.rail) {
        case (.in, .card):
            String(localized: .Funding.endedPaid)
        case (.in, _):
            String(localized: .Funding.endedSent)
        case (.out, _) where ended.succeeded:
            String(localized: .Funding.endedReceived)
        case (.out, _):
            String(localized: .Funding.endedWithdrawalAmount)
        }
    }

    var settledCaption: String {
        ended.direction == .in ? String(localized: .Funding.endedCredited) : String(localized: .Funding.endedDebited)
    }

    var endedCaption: String {
        ended.succeeded ? String(localized: .Funding.endedCompleted) : String(localized: .Funding.endedEnded)
    }

    /// How the user paid or was paid, with the network for crypto.
    var method: String? {
        guard let rail = ended.record.rail else { return nil }
        switch rail {
        case .card:
            return String(localized: .Funding.railCard)
        case .bank:
            return String(localized: .Funding.endedMethodBank)
        case .crypto:
            let crypto = String(localized: .Funding.railCrypto)
            guard let network = nonEmpty(ended.record.paidNetwork) else { return crypto }
            let name = FundingNetwork(id: network).name
            return String(localized: .Funding.endedMethodNetwork(method: crypto, network: name))
        }
    }

    var payoutText: String? {
        switch ended.payoutState {
        case .pending: String(localized: .Funding.endedPayoutPending)
        case .paidOut: String(localized: .Funding.endedPayoutPaidOut)
        case .failed: String(localized: .Funding.endedPayoutFailedStatus)
        case nil: nil
        }
    }

    func dateTime(_ date: Date) -> String {
        date.formatted(date: .abbreviated, time: .shortened)
    }

    func nonEmpty(_ text: String?) -> String? {
        guard let text, !text.isEmpty else { return nil }
        return text
    }

    func signedFigure(_ value: Decimal) -> String {
        (ended.direction == .in ? "+" : "-") + cash.figure(value)
    }

    func value(_ text: String) -> some View {
        Text(verbatim: text)
            .typography(.bodyLargeEmphasized)
            .foregroundStyle(.fgPrimary)
            .multilineTextAlignment(.trailing)
    }

    func loadProviderName() async {
        guard let providerId = ended.record.providerId else { return }
        providerName = await branding.brand(for: providerId).name
    }
}
