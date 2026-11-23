import DesignSystem
import FoundationExt
import PolkadotUI
import SubstrateSdk
import SwiftUI
import TrUAPIHost

/// Where a started session stands: the amount, the steps the core reports for
/// its direction and rail, and what the user may still need, such as the bank
/// details to pay to. Once the session ends it shows the ended screen.
struct FundingProgressView: View {
    let viewModel: FundingProgressViewModel
    let presenter: FundingProgressPresenterProtocol
    let branding: FundingProviderBranding


    var body: some View {
        Group {
            if let ended = viewModel.ended {
                FundingEndedView(
                    ended: ended.session,
                    viewModel: ended.viewModel,
                    branding: branding,
                    onFees: presenter.showFees,
                    onCopy: presenter.didCopy,
                    onClose: presenter.close,
                    onStartOver: presenter.startOver
                )
            } else {
                inProgress
            }
        }
        .sheet(isPresented: offerShown) {
            if let offer = viewModel.cancelOffer {
                FundingCancelOfferView(offer: offer, isAnswering: viewModel.isAnsweringOffer, presenter: presenter)
                    .presentationDetents([.medium])
                    .interactiveDismissDisabled()
            }
        }
        .fundingScreen()
    }
}

// MARK: - Sections

private extension FundingProgressView {
    /// The provider's cancel offer stays up until the user answers it.
    var offerShown: Binding<Bool> {
        Binding(get: { viewModel.cancelOffer != nil }, set: { _ in })
    }

    var inProgress: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: "", onBack: presenter.close)

            ScrollView {
                VStack(spacing: DSSpacings.medium) {
                    headline
                    stepCard
                    rows
                }
            }

            if let cancelTitle = viewModel.cancelTitle {
                DSButton(cancelTitle, style: .destructive, expands: true, action: presenter.confirmCancel)
            }
        }
    }

    var headline: some View {
        VStack(spacing: DSSpacings.small) {
            Image(systemName: "arrow.triangle.2.circlepath")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(viewModel.isSlow ? Color.fgWarning : Color.fgPrimary)
                .frame(width: 40, height: 40)
                .background(.bgSurfaceMain, in: Circle())
            if let figure = viewModel.figure {
                DSAmount(
                    amount: figure,
                    symbol: viewModel.symbol,
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
            FundingStepBar(steps: viewModel.steps, between: viewModel.stepsBetween)
                .padding(.horizontal, DSSpacings.mediumIncreased)
                .padding(.vertical, DSSpacings.medium)
                .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))

            if let note = viewModel.note {
                VStack(spacing: 2) {
                    Text(verbatim: note.text)
                        .typography(.bodySmall)
                        .foregroundStyle(note.isWarning ? Color.fgWarning : Color.fgSecondary)
                    if let liveLine = viewModel.liveLine {
                        FundingLiveLine(heardAt: liveLine.heardAt, detail: liveLine.detail)
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

    var rows: some View {
        VStack(spacing: DSSpacings.extraSmall) {
            switch viewModel.details {
            case let .amount(title, amount):
                feesRow(title, amount)
            case let .bank(bank):
                bankRows(bank)
            case let .crypto(deposit):
                cryptoRow(deposit)
            }
            arrivesRow
        }
    }

    @ViewBuilder
    func bankRows(_ bank: FundingProgressViewModel.Bank) -> some View {
        feesRow(bank.amountTitle, bank.amount)
        ForEach(bank.copyLines, id: \.title) { line in
            copyRow(line.title, line.text)
        }
        if bank.isPreparing {
            pending(.Funding.depositPreparing)
        }
    }

    /// While the funds are not seen, the way back to the address to pay.
    @ViewBuilder
    func cryptoRow(_ deposit: FundingProgressViewModel.CryptoDeposit) -> some View {
        switch deposit {
        case .hidden:
            EmptyView()
        case .preparing:
            pending(.Funding.depositPreparing)
        case let .deposit(title, amount):
            FundingValueRow(title: title, action: presenter.showDeposit) {
                amount.map { value($0) }
            }
        }
    }

    /// When the money lands; a top-up's Cash follows the rate on arrival.
    @ViewBuilder
    var arrivesRow: some View {
        if let arrival = viewModel.arrival {
            VStack(alignment: .trailing, spacing: 2) {
                FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                    value(arrival.eta)
                }
                if let rateNote = arrival.rateNote {
                    Text(verbatim: rateNote)
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
        let showFees: (() -> Void)? = viewModel.hasFees ? { presenter.showFees() } : nil
        return FundingValueRow(title: title, action: showFees) {
            if let amount {
                value(amount)
            } else {
                FundingSkeleton()
            }
        }
    }

    func copyRow(_ title: String, _ text: String) -> some View {
        FundingCopyRow(title: title, text: text, onCopy: presenter.didCopy)
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
}

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
        let reportedAt = detail?.atMs.map(Date.init(milliseconds:))
        let parts: [String?] =
            if let fact = detail?.fact, fact != .checked {
                parts(of: fact, at: reportedAt, now: now)
            } else {
                [(reportedAt ?? heardAt).map { text(heardAt: $0, now: now) }]
            }
        let line = parts.compactMap(\.self).joined(separator: " · ")
        if line.isEmpty, let heardAt { return text(heardAt: heardAt, now: now) }
        return line.isEmpty ? nil : line
    }

    /// What the provider reported, then how long ago it was or how long is
    /// left until it is due.
    static func parts(of fact: FundingFact, at reportedAt: Date?, now: Date) -> [String?] {
        let ago = reportedAt.flatMap { $0 <= now ? duration(Int(now.timeIntervalSince($0))) : nil }
        let left = reportedAt.flatMap { $0 > now ? duration(Int($0.timeIntervalSince(now).rounded(.up))) : nil }
        switch fact {
        case .checked:
            return []
        case let .confirmations(done, required):
            return [
                String(localized: .Funding.liveConfirmation(done: "\(done)", required: "\(required)")),
                ago.map { String(localized: .Funding.liveLastBlock(time: $0)) }
            ]
        case let .parts(done, total):
            return [
                String(localized: .Funding.livePart(done: "\(done)", total: "\(total)")),
                ago.map { String(localized: .Funding.liveLastPart(time: $0)) }
            ]
        case let .attempt(attempt, attempts):
            return [
                attempts.map { String(localized: .Funding.liveAttemptOf(n: "\(attempt)", of: "\($0)")) }
                    ?? String(localized: .Funding.liveAttempt(n: "\(attempt)")),
                left.map { String(localized: .Funding.liveNextTry(time: $0)) }
            ]
        case .scheduled,
             .sentByBank,
             .expectedBy:
            return [timed(fact, at: reportedAt, left: left)]
        case .priceChecked,
             .inReview,
             .other:
            return [labelled(fact), ago.map { String(localized: .Funding.liveAgo(time: $0)) }]
        }
    }

    /// A fact read against a moment: when it happened or when it is due.
    static func timed(_ fact: FundingFact, at reportedAt: Date?, left: String?) -> String? {
        switch fact {
        case .scheduled:
            left.map { String(localized: .Funding.liveTimeLeft(time: $0)) } ?? String(localized: .Funding.liveAnyMoment)
        case .sentByBank:
            [String(localized: .Funding.liveSentByBank), reportedAt.map(moment)]
                .compactMap(\.self)
                .joined(separator: " · ")
        case .expectedBy:
            reportedAt.map {
                String(localized: .Funding.liveExpectedBy(date: $0.formatted(.dateTime.weekday().day().month())))
            }
        default:
            nil
        }
    }

    /// A fact's own words, before its age.
    static func labelled(_ fact: FundingFact) -> String? {
        switch fact {
        case .priceChecked: String(localized: .Funding.livePriceChecked)
        case .inReview: String(localized: .Funding.liveInReview)
        case let .other(text): text
        default: nil
        }
    }

    static func text(heardAt: Date, now: Date) -> String {
        let seconds = max(0, Int(now.timeIntervalSince(heardAt)))
        guard seconds >= justNowBelow else { return String(localized: .Funding.liveCheckedNow) }
        return String(localized: .Funding.liveChecked(time: duration(seconds)))
    }

    /// "12s", "3 min" or "2 h".
    static func duration(_ seconds: Int) -> String {
        switch seconds {
        case ..<60: String(localized: .Funding.liveSeconds(count: "\(seconds)"))
        case ..<3_600: String(localized: .Funding.liveMinutes(count: "\(seconds / 60)"))
        default: String(localized: .Funding.liveHours(count: "\(seconds / 3_600)"))
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
    let offer: FundingProgressViewModel.CancelOffer
    let isAnswering: Bool
    let presenter: FundingProgressPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            Text(offer.title)
                .typography(.headlineMedium)
                .foregroundStyle(.fgPrimary)
            Text(.Funding.offerBody)
                .typography(.bodyMedium)
                .foregroundStyle(.fgSecondary)
                .multilineTextAlignment(.center)

            VStack(spacing: DSSpacings.small) {
                row(String(localized: .Funding.offerFee), offer.fee)
                row(String(localized: .Funding.offerRefund), offer.refund)
            }
            .padding(DSSpacings.medium)
            .background(.bgSurfaceNested, in: RoundedRectangle(cornerRadius: 16))

            Spacer(minLength: 0)

            DSButton(.Funding.offerAccept, style: .destructive, expands: true, isLoading: isAnswering) {
                presenter.answerCancelOffer(accepting: offer.terms)
            }
            DSButton(.Funding.offerKeepGoing, expands: true) {
                presenter.answerCancelOffer(accepting: nil)
            }
        }
        .padding(DSSpacings.medium)
    }

    private func row(_ title: String, _ amount: String) -> some View {
        HStack {
            Text(verbatim: title)
                .typography(.bodyMedium)
                .foregroundStyle(.fgSecondary)
            Spacer()
            Text(verbatim: amount)
                .typography(.bodyMediumEmphasized)
                .foregroundStyle(.fgPrimary)
        }
    }
}
