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

    /// The band line, worded for the session's rail and amount.
    var band: (text: String, isWarning: Bool)? {
        guard let progress = model.progress else { return nil }
        return FundingWording.band(
            progress.band,
            direction: model.direction,
            rail: model.rail,
            asset: model.asset,
            amount: amountUnits.map { model.cash.label(model.cash.decimal($0)) }
        )
    }

    /// Retrying or delayed: the session's dot and line turn amber.
    var isSlow: Bool {
        model.progress.map { [.retrying, .delayed].contains($0.band) } ?? false
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
                    .current(isAmber: isSlow)
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

    /// The line under the steps: where a cancel stands, else the band's.
    var note: (text: String, color: Color)? {
        if let status = model.progress?.status, let line = FundingCancelPresentation.line(
            for: status,
            direction: model.direction
        ) {
            return (line.text, line.isWarning ? .fgWarning : .fgSecondary)
        }
        if let status = model.progress?.status, FundingCancelPresentation.cancelCameTooLate(status) {
            return (String(localized: .Funding.cancelCameTooLate), .fgWarning)
        }
        return band.map { ($0.text, $0.isWarning ? .fgWarning : .fgSecondary) }
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
                .foregroundStyle(isSlow ? Color.fgWarning : Color.fgPrimary)
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
            FundingStepBar(steps: steps, between: FundingStepBar.between(model.progress?.band))
                .padding(.horizontal, DSSpacings.mediumIncreased)
                .padding(.vertical, DSSpacings.medium)
                .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))

            if let note {
                VStack(spacing: 2) {
                    Text(verbatim: note.text)
                        .typography(.bodySmall)
                        .foregroundStyle(note.color)
                    if let heardAtMs = model.progress?.lastHeardAtMs {
                        FundingLiveLine(heardAt: Date(milliseconds: heardAtMs))
                    }
                }
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
                feesRow(String(localized: .Funding.progressYouSent), quotedSendAmount)
                arrivesRow
            case (.in, .bank):
                bankRows
            case (.in, .crypto):
                cryptoRow
                arrivesRow
            case (.out, _):
                feesRow(
                    String(localized: .Funding.summaryWithdrawCaption),
                    model.chosenQuote.map { model.quoteUnit($0).format($0.receiveAmount) }
                )
                arrivesRow
            }
        }
    }

    /// "Send this exact amount" until the transfer is seen, then what was sent.
    var bankAmountTitle: String {
        model.hasReached(.received)
            ? String(localized: .Funding.progressYouSent)
            : String(localized: .Funding.summaryBankCaption)
    }

    @ViewBuilder
    var bankRows: some View {
        if case let .bank(amount, currency, decimals, beneficiary, account, bankCode, reference, _)? =
            model.progress?.deposit {
            feesRow(bankAmountTitle, FundingAssetUnit(code: currency, decimals: decimals).format(amount))
            beneficiary.map { copyRow(String(localized: .Funding.depositBeneficiary), $0) }
            account.map { copyRow(String(localized: .Funding.depositAccount), $0) }
            bankCode.map { copyRow(String(localized: .Funding.depositBankCode), $0) }
            copyRow(String(localized: .Funding.depositReference), reference)
        } else {
            feesRow(bankAmountTitle, quotedSendAmount)
            if let reference = model.progress?.reference {
                copyRow(String(localized: .Funding.depositReference), reference)
            } else if !model.hasReached(.received) {
                pending(.Funding.depositPreparing)
            }
        }
        arrivesRow
    }

    /// While the funds are not seen, the way back to the address to pay.
    @ViewBuilder
    var cryptoRow: some View {
        if !model.hasReached(.received) {
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

    /// When the money lands; a top-up's Cash follows the rate on arrival.
    @ViewBuilder
    var arrivesRow: some View {
        if let eta = model.chosenQuote?.etaSecs {
            VStack(alignment: .trailing, spacing: 2) {
                FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                    value(FundingEta.text(seconds: eta))
                }
                if model.direction == .in, model.rail != .card {
                    Text(.Funding.summaryBankRateNote(symbol: model.cash.symbol))
                        .typography(.captionSmall)
                        .foregroundStyle(.fgSecondary)
                }
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

/// A session's timeline: its three milestones, labelled, with two dots
/// between each that fill as the session moves. Done milestones are a green
/// check joined in green, the dot the session is at is white (amber when it is
/// slow), upcoming ones grey, and a failed milestone red.
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
    /// How far the session is past the last done milestone: 0 at it, then 1
    /// or 2 for the dots towards the next.
    var between = 1

    private static let size: CGFloat = 22
    private static let dot: CGFloat = 8

    /// Which dot towards the next milestone a band stands at.
    static func between(_ band: FundingBand?) -> Int {
        switch band {
        case .checkingPayment,
             .adding,
             .sending: 2
        default: 1
        }
    }

    /// A milestone's name; a withdrawal's second one is the provider's.
    static func title(_ step: FundingStep, direction: FundingDirection) -> String {
        switch step {
        case .started: String(localized: .Funding.progressStepStarted)
        case .received:
            String(localized: direction == .out ? .Funding.progressStepReceivedOut : .Funding.progressStepReceived)
        case .added: String(localized: .Funding.progressStepAdded)
        case .sent: String(localized: .Funding.progressStepSent)
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                if index > 0 {
                    segment(into: step.state)
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
        .padding(.horizontal, DSSpacings.medium)
        .animation(.easeInOut, value: steps)
        .animation(.easeInOut, value: between)
    }
}

private extension FundingStepBar {
    enum DotState {
        case done
        case current(isAmber: Bool)
        case upcoming
    }

    @ViewBuilder
    func mark(_ state: State) -> some View {
        switch state {
        case .done:
            symbol("checkmark", color: .white)
                .background(Color.fgSuccess, in: Circle())
        case .current,
             .upcoming:
            symbol("checkmark", color: .fgTertiary)
                .background(Color.bgSurfaceNested, in: Circle())
        case .failed:
            symbol("xmark", color: .white)
                .background(Color.fgError, in: Circle())
        }
    }

    func symbol(_ name: String, color: Color) -> some View {
        Image(systemName: name)
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(color)
            .frame(width: Self.size, height: Self.size)
    }

    /// The line and two dots leading into a milestone in `state`.
    func segment(into state: State) -> some View {
        let dots = dots(into: state)
        return HStack(spacing: 0) {
            line(filled: dots[0].isFilled)
            dotMark(dots[0])
            line(filled: dots[1].isFilled)
            dotMark(dots[1])
            line(filled: state == .done)
        }
    }

    /// The two dots before a milestone: both done once it is reached, the
    /// session's position on the way to the current one, grey after it.
    func dots(into state: State) -> [DotState] {
        switch state {
        case .done:
            [.done, .done]
        case let .current(isAmber):
            (0 ..< 2).map { index in
                if index < between - 1 { return .done }
                return index == max(between - 1, 0) ? .current(isAmber: isAmber) : .upcoming
            }
        case .upcoming,
             .failed:
            [.upcoming, .upcoming]
        }
    }

    func dotMark(_ state: DotState) -> some View {
        Circle()
            .fill(dotColor(state))
            .frame(width: Self.dot, height: Self.dot)
    }

    func dotColor(_ state: DotState) -> Color {
        switch state {
        case .done: .fgSuccess
        case let .current(isAmber): isAmber ? .fgWarning : .fgPrimary
        case .upcoming: .bgSurfaceNested
        }
    }

    func line(filled: Bool) -> some View {
        Rectangle()
            .fill(filled ? Color.fgSuccess : Color.bgSurfaceNested)
            .frame(height: 2)
            .frame(maxWidth: .infinity)
    }

    func labelColor(_ state: State) -> Color {
        switch state {
        case .done: .fgSuccess
        case .current,
             .upcoming: .fgTertiary
        case .failed: .fgError
        }
    }
}

private extension FundingStepBar.DotState {
    var isFilled: Bool {
        if case .done = self { return true }
        return false
    }
}

/// When the provider was last heard from, kept current while on screen:
/// "Checked with the provider 12s ago", then "Trying to reconnect" once a
/// minute has passed without word.
struct FundingLiveLine: View {
    let heardAt: Date

    static let staleAfter: TimeInterval = 60

    var body: some View {
        TimelineView(.periodic(from: .now, by: 3)) { context in
            Text(verbatim: Self.text(heardAt: heardAt, now: context.date))
                .typography(.captionSmall)
                .foregroundStyle(.fgTertiary)
        }
    }

    static func text(heardAt: Date, now: Date) -> String {
        let elapsed = max(0, now.timeIntervalSince(heardAt))
        guard elapsed < staleAfter else { return String(localized: .Funding.liveReconnecting) }
        return String(localized: .Funding.liveChecked(time: "\(Int(elapsed))s"))
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
            Text(model.direction == .out ? .Funding.offerTitleWithdraw : .Funding.offerTitle)
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
