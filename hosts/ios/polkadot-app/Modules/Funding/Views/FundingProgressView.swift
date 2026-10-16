import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Where a started session stands: the amount, the steps the core reports for
/// its direction and rail, and what the user may still need, such as the bank
/// details to pay to. Once the session ends it shows the ended screen.
struct FundingProgressView: View {
    let model: FundingFlowModel

    @State private var copied: String?

    var body: some View {
        Group {
            if let ended = model.ended {
                FundingEndedView(
                    ended: ended,
                    cash: model.cash,
                    branding: model.branding,
                    onFees: model.showFees,
                    onClose: model.close,
                    onStartOver: model.startOver
                )
            } else {
                inProgress
            }
        }
        .task(id: model.progress?.longerAtMs) { await follow() }
        .sheet(isPresented: offerShown) {
            if let offer = model.cancelOffer {
                FundingCancelOfferView(model: model, fee: offer.fee, refund: offer.refund)
                    .presentationDetents([.medium])
                    .interactiveDismissDisabled()
            }
        }
    }
}

// MARK: - State

private extension FundingProgressView {
    /// The CASH card row for the session, for its amount.
    var item: FundingActivityItem? {
        model.session.map { FundingActivityItem(session: $0, progress: model.progress) }
    }

    /// Slower than its quote said, which the core reports as taking longer.
    var isDelayed: Bool {
        model.progress?.status == .takingLonger
    }

    var isRetrying: Bool {
        model.progress?.retrying == true
    }

    var amountUnits: U128? {
        item?.amount ?? model.chosenQuote.map { model.direction == .in ? $0.receiveAmount : $0.sendAmount }
    }

    var steps: [FundingStepBar.Step] {
        let reported = model.progress?.steps ?? []
        let firstUnreached = reported.firstIndex { $0.reachedAtMs == nil }

        return reported.enumerated().map { index, step in
            let state: FundingStepBar.State =
                if step.reachedAtMs != nil {
                    .done
                } else if index == firstUnreached {
                    .current(isAmber: isDelayed || isRetrying)
                } else {
                    .upcoming
                }
            return FundingStepBar.Step(title: FundingStepBar.title(step.step, direction: model.direction), state: state)
        }
    }

    /// The provider's cancel offer stays up until the user answers it.
    var offerShown: Binding<Bool> {
        Binding(get: { model.cancelOffer != nil }, set: { _ in })
    }

    /// The line under the steps: where a cancel stands, why it is slow, or
    /// for a bank transfer not seen yet, that it is on its way.
    var note: (text: String, color: Color)? {
        if let status = model.progress?.status, let line = FundingCancelPresentation.line(for: status) {
            return (line.text, line.isWarning ? .fgWarning : .fgSecondary)
        }
        if let status = model.progress?.status,
           FundingCancelPresentation
           .cancelCameTooLate(status, accepted: FundingAcceptedCancels.contains(model.intent)) {
            return (String(localized: .Funding.cancelCameTooLate), .fgWarning)
        }
        if isRetrying { return (String(localized: .Funding.activityRetrying), .fgWarning) }
        if isDelayed { return (String(localized: .Funding.activityDelayed), .fgWarning) }
        if model.direction == .in, model.rail == .bank, !model.hasReached(.payment) {
            return (String(localized: .Funding.progressBankNote), .fgSecondary)
        }
        return nil
    }

    var quotedSendAmount: String? {
        model.chosenQuote.map { model.quoteUnit($0).format($0.sendAmount) }
    }
}

// MARK: - Sections

private extension FundingProgressView {
    var inProgress: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: "", onBack: model.hasStarted ? model.close : model.back)

            ScrollView {
                VStack(spacing: DSSpacings.medium) {
                    headline
                    stepCard
                    rows
                }
            }

            if model.canRequestCancel {
                FundingPrimaryButton(
                    title: model.cancelTitle,
                    style: .destructive,
                    action: model.confirmCancel
                )
            }
        }
        .fundingToast($copied)
    }

    var headline: some View {
        VStack(spacing: DSSpacings.small) {
            Image(systemName: "arrow.triangle.2.circlepath")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(.fgPrimary)
                .frame(width: 40, height: 40)
                .background(.bgSurfaceMain, in: Circle())
            if let units = amountUnits {
                let figure = model.cash.figure(model.cash.decimal(units))
                DSAmount(
                    amount: figure,
                    symbol: model.cash.symbol,
                    typography: .fundingFigure(digits: figure.filter(\.isNumber).count)
                )
                .foregroundStyle(.fgPrimary)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
            }
        }
        .padding(.top, DSSpacings.small)
    }

    var stepCard: some View {
        VStack(spacing: 0) {
            FundingStepBar(steps: steps)
                .padding(.horizontal, DSSpacings.mediumIncreased)
                .padding(.vertical, DSSpacings.medium)
                .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))

            if let note {
                Text(verbatim: note.text)
                    .typography(.bodySmall)
                    .foregroundStyle(note.color)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, DSSpacings.medium)
                    .padding(.vertical, DSSpacings.smallIncreased)
            }
        }
        .background(.bgSurfaceNested, in: RoundedRectangle(cornerRadius: DSRadii.large))
    }

    @ViewBuilder
    var rows: some View {
        VStack(spacing: DSSpacings.extraSmall) {
            switch (model.direction, model.rail) {
            case (.in, .card):
                feesRow(String(localized: .Funding.progressYouPaid), quotedSendAmount)
            case (.in, .bank):
                bankRows
            case (.in, .crypto):
                cryptoRow
            case (.out, _):
                feesRow(
                    String(localized: .Funding.summaryWithdrawCaption),
                    model.chosenQuote.map { model.quoteUnit($0).format($0.receiveAmount) }
                )
                arrivesRow
            }
        }
    }

    @ViewBuilder
    var bankRows: some View {
        if case let .bank(amount, currency, decimals, beneficiary, account, bankCode, reference, _)? =
            model.progress?.deposit {
            feesRow(
                String(localized: .Funding.summaryBankCaption),
                FundingAssetUnit(code: currency, decimals: decimals).format(amount)
            )
            beneficiary.map { copyRow(String(localized: .Funding.depositBeneficiary), $0) }
            account.map { copyRow(String(localized: .Funding.depositAccount), $0) }
            bankCode.map { copyRow(String(localized: .Funding.depositBankCode), $0) }
            copyRow(String(localized: .Funding.depositReference), reference)
        } else {
            feesRow(String(localized: .Funding.summaryBankCaption), quotedSendAmount)
            if let reference = model.progress?.reference {
                copyRow(String(localized: .Funding.depositReference), reference)
            } else if !model.hasReached(.payment) {
                pending(.Funding.depositPreparing)
            }
        }
        arrivesRow
    }

    /// While the funds are not seen, the way back to the address to pay.
    @ViewBuilder
    var cryptoRow: some View {
        if !model.hasReached(.payment) {
            if case let .crypto(_, _, asset, amount, decimals, exact, _, _)? = model.progress?.deposit {
                FundingValueRow(
                    title: String(localized: exact ? .Funding.depositExactAmount : .Funding.depositAtLeast),
                    action: model.showDeposit
                ) {
                    value(FundingAssetUnit(code: asset, decimals: decimals).format(amount))
                }
            } else {
                pending(.Funding.depositPreparing)
            }
        }
    }

    @ViewBuilder
    var arrivesRow: some View {
        if let eta = model.chosenQuote?.etaSecs {
            FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                value(FundingEta.text(seconds: eta))
            }
        }
    }
}

// MARK: - Rows

private extension FundingProgressView {
    func feesRow(_ title: String, _ amount: String?) -> some View {
        let showFees: (() -> Void)? = model.chosenQuote == nil ? nil : { model.showFees() }
        return FundingValueRow(title: title, action: showFees) {
            if let amount {
                value(amount)
            } else {
                FundingSkeleton()
            }
        }
    }

    func copyRow(_ title: String, _ text: String) -> some View {
        FundingCopyRow(title: title, text: text, copied: $copied)
    }

    func value(_ text: String) -> some View {
        Text(verbatim: text)
            .typography(.bodyLargeEmphasized)
            .foregroundStyle(.fgPrimary)
            .multilineTextAlignment(.trailing)
    }

    func pending(_ text: LocalizedStringResource) -> some View {
        HStack(spacing: DSSpacings.small) {
            ProgressView().controlSize(.small).tint(.fgSecondary)
            Text(text)
                .typography(.bodySmall)
                .foregroundStyle(.fgSecondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, DSSpacings.small)
    }

    /// The core's change events keep the screen current, except for a
    /// session turning to taking longer, which no report announces: read the
    /// session again at the time the core gave for it.
    func follow() async {
        guard model.hasStarted, let longerAtMs = model.progress?.longerAtMs else { return }

        let wait = Date(milliseconds: longerAtMs).timeIntervalSinceNow
        if wait > 0 {
            try? await Task.sleep(for: .seconds(wait))
        }
        guard !Task.isCancelled else { return }
        model.refreshSession()
    }
}

/// The steps of a session in a row: done steps a green check joined in green,
/// the current one a spinner, upcoming ones grey, and a failed one red.
struct FundingStepBar: View {
    enum State: Equatable {
        case done
        case current(isAmber: Bool)
        case upcoming
        case failed
    }

    struct Step: Equatable {
        let title: String
        let state: State
    }

    let steps: [Step]

    private static let size: CGFloat = 28

    /// A step's name: a withdrawal's read as the money leaves, reaches the
    /// provider, is prepared and paid out.
    static func title(_ step: FundingStep, direction: FundingDirection) -> String {
        let out = direction == .out
        let payment: LocalizedStringResource = out ? .Funding.progressStepSending : .Funding.progressStepPayment
        let conversion: LocalizedStringResource = out ? .Funding.progressStepPreparing : .Funding.progressStepConversion
        return switch step {
        case .started: String(localized: .Funding.progressStepStarted)
        case .payment: String(localized: payment)
        case .approved: String(localized: .Funding.progressStepApproved)
        case .conversion: String(localized: conversion)
        case .added: String(localized: .Funding.progressStepAdded)
        case .sent: String(localized: .Funding.progressStepReceived)
        case .payout: String(localized: .Funding.progressStepPayout)
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                if index > 0 {
                    connector(to: step.state)
                }
                mark(step.state)
                    .overlay(alignment: .top) {
                        Text(verbatim: step.title)
                            .typography(.labelSmallEmphasized)
                            .foregroundStyle(labelColor(step.state))
                            .lineLimit(1)
                            .fixedSize()
                            .offset(y: Self.size + DSSpacings.extraSmall)
                    }
            }
        }
        .padding(.bottom, 20)
        .padding(.horizontal, DSSpacings.small)
        .animation(.easeInOut, value: steps)
    }
}

private extension FundingStepBar {
    @ViewBuilder
    func mark(_ state: State) -> some View {
        switch state {
        case .done:
            symbol("checkmark", color: .white)
                .background(Color.fgSuccess, in: Circle())
        case let .current(isAmber):
            ProgressView()
                .controlSize(.small)
                .tint(Color.fgPrimaryInverted)
                .frame(width: Self.size, height: Self.size)
                .background(isAmber ? Color.fgWarning : Color.fgPrimary, in: Circle())
        case .upcoming:
            symbol("checkmark", color: .fgTertiary)
                .background(Color.bgSurfaceNested, in: Circle())
        case .failed:
            symbol("xmark", color: .white)
                .background(Color.fgError, in: Circle())
        }
    }

    func symbol(_ name: String, color: Color) -> some View {
        Image(systemName: name)
            .font(.system(size: 12, weight: .bold))
            .foregroundStyle(color)
            .frame(width: Self.size, height: Self.size)
    }

    /// Green into a done step, half green into the current one, grey after.
    func connector(to state: State) -> some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.bgSurfaceNested)
                Capsule()
                    .fill(Color.fgSuccess)
                    .frame(width: proxy.size.width * fill(state))
            }
        }
        .frame(height: 3)
        .padding(.horizontal, DSSpacings.extraSmall)
    }

    func fill(_ state: State) -> CGFloat {
        switch state {
        case .done: 1
        case .current: 0.5
        case .upcoming,
             .failed: 0
        }
    }

    func labelColor(_ state: State) -> Color {
        switch state {
        case .done: .fgSuccess
        case let .current(isAmber): isAmber ? .fgWarning : .fgPrimary
        case .upcoming: .fgTertiary
        case .failed: .fgError
        }
    }
}

/// The provider's terms for cancelling: what it keeps and what comes back to
/// the balance, accepted or declined by the user.
struct FundingCancelOfferView: View {
    let model: FundingFlowModel
    let fee: U128
    let refund: U128

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            Text(.Funding.offerTitle)
                .typography(.headlineMedium)
                .foregroundStyle(.fgPrimary)
            Text(.Funding.offerBody)
                .typography(.bodyMedium)
                .foregroundStyle(.fgSecondary)
                .multilineTextAlignment(.center)

            VStack(spacing: DSSpacings.small) {
                row(String(localized: .Funding.offerFee), fee)
                row(String(localized: .Funding.offerRefund), refund)
            }
            .padding(DSSpacings.medium)
            .background(.bgSurfaceNested, in: RoundedRectangle(cornerRadius: 16))

            Spacer(minLength: 0)

            FundingPrimaryButton(
                title: String(localized: .Funding.offerAccept),
                style: .destructive,
                isLoading: model.isAnsweringOffer
            ) {
                model.answerCancelOffer(accepting: CancelTerms(fee: fee, refund: refund))
            }
            FundingPrimaryButton(title: String(localized: .Funding.offerKeepGoing)) {
                model.answerCancelOffer(accepting: nil)
            }
        }
        .padding(DSSpacings.medium)
    }

    private func row(_ title: String, _ units: U128) -> some View {
        HStack {
            Text(verbatim: title)
                .typography(.bodyMedium)
                .foregroundStyle(.fgSecondary)
            Spacer()
            Text(verbatim: model.cash.label(model.cash.decimal(units)))
                .typography(.bodyMediumEmphasized)
                .foregroundStyle(.fgPrimary)
        }
    }
}
