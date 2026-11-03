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
                    if let progress = model.progress, progress.lastHeardAtMs != nil || progress.detail != nil {
                        FundingLiveLine(
                            heardAt: progress.lastHeardAtMs.map(Date.init(milliseconds:)),
                            detail: progress.detail
                        )
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

/// When the provider was last heard from, kept current while on screen:
/// "just now", then seconds, minutes and hours.
/// The line under a session's status: the provider's latest fact for the
/// current step, worded here, or when the provider was last heard. One line,
/// cut short with "…" when it does not fit.
struct FundingLiveLine: View {
    let heardAt: Date?
    let detail: FundingDetail?

    static let justNowBelow = 5

    var body: some View {
        TimelineView(.periodic(from: .now, by: 3)) { context in
            if let text = Self.text(detail: detail, heardAt: heardAt, now: context.date) {
                Text(verbatim: text)
                    .typography(.captionSmall)
                    .foregroundStyle(.fgTertiary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    static func text(detail: FundingDetail?, heardAt: Date?, now: Date) -> String? {
        let at = detail?.atMs.map(Date.init(milliseconds:))
        let ago = at.flatMap { $0 <= now ? duration(Int(now.timeIntervalSince($0))) : nil }
        let left = at.flatMap { $0 > now ? duration(Int($0.timeIntervalSince(now).rounded(.up))) : nil }
        let parts: [String?] =
            switch detail?.fact {
            case .none,
                 .checked?:
                [(at ?? heardAt).map { text(heardAt: $0, now: now) }]
            case let .confirmations(done, required)?:
                [
                    String(localized: .Funding.liveConfirmation(done: "\(done)", required: "\(required)")),
                    ago.map { String(localized: .Funding.liveLastBlock(time: $0)) },
                ]
            case .priceChecked?:
                [
                    String(localized: .Funding.livePriceChecked),
                    ago.map { String(localized: .Funding.liveAgo(time: $0)) }
                ]
            case let .parts(done, total)?:
                [
                    String(localized: .Funding.livePart(done: "\(done)", total: "\(total)")),
                    ago.map { String(localized: .Funding.liveLastPart(time: $0)) },
                ]
            case .scheduled?:
                [left
                    .map { String(localized: .Funding.liveTimeLeft(time: $0)) } ??
                    String(localized: .Funding.liveAnyMoment)]
            case .sentByBank?:
                [String(localized: .Funding.liveSentByBank), at.map(moment)]
            case .expectedBy?:
                [at
                    .map {
                        String(localized: .Funding
                            .liveExpectedBy(date: $0.formatted(.dateTime.weekday().day().month())))
                    }]
            case .inReview?:
                [String(localized: .Funding.liveInReview), ago.map { String(localized: .Funding.liveAgo(time: $0)) }]
            case let .attempt(attempt, attempts)?:
                [
                    attempts.map { String(localized: .Funding.liveAttemptOf(n: "\(attempt)", of: "\($0)")) }
                        ?? String(localized: .Funding.liveAttempt(n: "\(attempt)")),
                    left.map { String(localized: .Funding.liveNextTry(time: $0)) },
                ]
            case let .other(text)?:
                [text, ago.map { String(localized: .Funding.liveAgo(time: $0)) }]
            }
        let line = parts.compactMap(\.self).joined(separator: " · ")
        if line.isEmpty, let heardAt { return text(heardAt: heardAt, now: now) }
        return line.isEmpty ? nil : line
    }

    static func text(heardAt: Date, now: Date) -> String {
        let seconds = max(0, Int(now.timeIntervalSince(heardAt)))
        guard seconds >= justNowBelow else { return String(localized: .Funding.liveCheckedNow) }
        return String(localized: .Funding.liveChecked(time: duration(seconds)))
    }

    /// "12s", "3 min" or "2 h".
    static func duration(_ seconds: Int) -> String {
        switch seconds {
        case ..<60: "\(seconds)s"
        case ..<3_600: "\(seconds / 60) min"
        default: "\(seconds / 3_600) h"
        }
    }

    /// "14:02" today, "Tue 14:02" otherwise.
    static func moment(_ date: Date) -> String {
        Calendar.current.isDateInToday(date)
            ? date.formatted(date: .omitted, time: .shortened)
            : date.formatted(.dateTime.weekday().hour().minute())
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
