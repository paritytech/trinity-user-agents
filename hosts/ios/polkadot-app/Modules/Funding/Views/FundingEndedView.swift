import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// An ended session's detail screen. A success shows what moved and when; a
/// failure shows the step it stopped on, why, and who to quote to support,
/// with a way to start a new session.
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

    var rows: some View {
        VStack(spacing: DSSpacings.extraSmall) {
            if let paid = ended.quotedAmount {
                FundingValueRow(title: amountCaption, action: onFees) {
                    value(paid)
                }
            }

            if ended.succeeded {
                if ended.direction == .out, let eta = ended.quote?.etaSecs {
                    FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                        value(FundingEta.text(seconds: eta))
                    }
                }
            } else {
                if ended.direction == .in, ended.record.rail == .bank, let reference = ended.record.reference {
                    FundingCopyRow(
                        title: String(localized: .Funding.depositReference),
                        text: reference,
                        copied: $copied
                    )
                }
                if let providerName {
                    FundingValueRow(title: String(localized: .Funding.summaryProvider)) {
                        value(providerName)
                    }
                }
                if let transactionId = ended.record.transactionId {
                    FundingCopyRow(
                        title: String(localized: .Funding.endedTransactionId),
                        text: transactionId,
                        copied: $copied,
                        truncatesMiddle: true
                    )
                }
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
            String(localized: .Funding.progressYouPaid)
        case (.out, .card) where !ended.succeeded:
            String(localized: .Funding.endedWithdrawalAmount)
        default:
            String(localized: .Funding.endedSent)
        }
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
