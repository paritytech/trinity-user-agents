import Foundation
import Foundation_iOS
import FoundationExt
import PolkadotUI
import SubstrateSdk
import TrUAPIHost

/// Turns what the funding screens draw into their view models. CASH is set as
/// the app sets the digital dollar figure, with its cents only when there are
/// any, followed by the payment asset's symbol; a provider's asset as the app
/// sets a price for a currency and a token balance for anything else.
protocol FundingViewModelFactoryProtocol {
    func makeAmountViewModel(for state: FundingEntryModel) -> FundingAmountViewModel
    func makeSummaryViewModel(for state: FundingEntryModel) -> FundingSummaryViewModel
    func makeFeesViewModel(for state: FundingEntryModel) -> FundingFeesViewModel
    func makeProvidersViewModel(for state: FundingEntryModel) -> FundingProvidersViewModel
    func makeNetworkViewModel(for state: FundingEntryModel) -> FundingNetworkViewModel
    func makeTokenViewModel(for state: FundingEntryModel) -> FundingTokenViewModel
    func makeCountryViewModel(for state: FundingEntryModel) -> FundingCountryViewModel
    func makeDepositViewModel(for state: FundingEntryModel) -> FundingDepositViewModel
    func makeProgressViewModel(for state: FundingEntryModel) -> FundingProgressViewModel
    func makeCancelConfirmViewModel(for state: FundingEntryModel) -> FundingCancelConfirmViewModel
    func makeEndedViewModel(for ended: FundingEndedSession) -> FundingEndedViewModel
    func makeActivityRow(for item: FundingActivityItem, now: Date) -> FundingActivityRowContent
}

final class FundingViewModelFactory {
    /// The top-up amounts offered under the figure, in dollars.
    private static let presets: [Decimal] = [10, 50, 100]

    let cashInfo: AssetBalanceDisplayInfo
    let cashSymbol: String
    let formatterFactory: AssetBalanceFormatterFactoryProtocol
    let timestampFormatter: TimestampFormatting
    let etaFormatter: TimeFormatterProtocol

    private let wholeCashFormatter: LocalizableDecimalFormatting
    private let centsCashFormatter: LocalizableDecimalFormatting

    init(
        cashInfo: AssetBalanceDisplayInfo,
        cashSymbol: String,
        formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory(),
        timestampFormatter: TimestampFormatting = FundingTimestampFormatter(),
        etaFormatter: TimeFormatterProtocol = FundingEtaFormatter()
    ) {
        self.cashInfo = cashInfo
        self.cashSymbol = cashSymbol
        self.formatterFactory = formatterFactory
        self.timestampFormatter = timestampFormatter
        self.etaFormatter = etaFormatter

        wholeCashFormatter = formatterFactory
            .createAssetPriceFormatter(for: cashInfo, minimumFractionDigits: 0)
            .value(for: .current)
        centsCashFormatter = formatterFactory
            .createAssetPriceFormatter(for: cashInfo, minimumFractionDigits: cashInfo.displayPrecision)
            .value(for: .current)
    }
}

// MARK: - Amounts

extension FundingViewModelFactory {
    /// "$50.20", with cents only when there are any.
    func cashFigure(_ amount: Balance) -> String {
        cashFigure(amount.decimal(assetInfo: cashInfo))
    }

    func cashFigure(_ value: Decimal) -> String {
        let formatter = value.hasFraction ? centsCashFormatter : wholeCashFormatter
        return formatter.stringFromDecimal(value) ?? ""
    }

    /// "$50.20 CASH".
    func cashLabel(_ amount: Balance) -> String {
        "\(cashFigure(amount)) \(cashSymbol)"
    }

    func cashLabel(units: U128?) -> String? {
        Balance(units: units).map(cashLabel)
    }

    /// "+$50" for value moving in, "-$50" for value moving out.
    func signedCashFigure(_ amount: Balance, direction: FundingDirection) -> String {
        (direction == .in ? "+" : "-") + cashFigure(amount)
    }

    /// "€25.10" or "51.8 USDT".
    func assetAmount(_ value: Decimal, info: AssetBalanceDisplayInfo) -> String {
        let formatter = info.symbolPosition == .prefix
            ? formatterFactory.createAssetPriceFormatter(
                for: info,
                minimumFractionDigits: value.hasFraction ? info.displayPrecision : 0
            )
            : formatterFactory.createTokenFormatter(for: info)
        return formatter.value(for: .current).stringFromDecimal(value) ?? ""
    }

    func assetAmount(_ amount: Balance, info: AssetBalanceDisplayInfo) -> String {
        assetAmount(amount.decimal(assetInfo: info), info: info)
    }

    func assetAmount(units: U128, asset: String, decimals: UInt8) -> String? {
        Balance(units: units).map { assetAmount($0, info: .from(assetCode: asset, decimals: Int16(decimals))) }
    }

    /// `units` of `quote`'s provider-side asset.
    func quoteAmount(_ units: U128, of quote: FundingQuote, in state: FundingEntryModel) -> String? {
        Balance(units: units).map { assetAmount($0, info: state.quoteUnit(quote)) }
    }

    func issueText(_ issue: FundingAmountIssue) -> String {
        switch issue {
        case let .belowMinimum(minimum):
            String(localized: .Funding.errorMinimum(amount: cashLabel(units: minimum) ?? ""))
        case let .aboveMaximum(maximum):
            String(localized: .Funding.errorMaximum(amount: cashLabel(units: maximum) ?? ""))
        case .notEnoughBalance:
            String(localized: .Funding.errorNotEnough(symbol: cashSymbol))
        }
    }

    var bankRateNote: String {
        String(localized: .Funding.summaryBankRateNote(symbol: cashSymbol))
    }
}

// MARK: - Screens

extension FundingViewModelFactory: FundingViewModelFactoryProtocol {
    func makeAmountViewModel(for state: FundingEntryModel) -> FundingAmountViewModel {
        let minimum = cashLabel(state.amountLimits.min)
        let minimumCaption = state.direction == .in
            ? String(localized: .Funding.amountMinimum(amount: minimum))
            : String(localized: .Funding.amountWithdrawMinimum(amount: minimum))
        let amount = state.amount ?? 0

        return FundingAmountViewModel(
            title: state.direction == .in
                ? String(localized: .Funding.amountAddTitle)
                : String(localized: .Funding.amountWithdrawTitle),
            history: state.historyClock,
            rail: state.rail,
            availableRails: state.availableRails,
            available: state.direction == .out ? state.spendable.map(cashLabel) : nil,
            amountText: state.amountText,
            figure: cashFigure(amount),
            hasAmount: amount > 0,
            symbol: cashSymbol,
            presets: state.direction == .in ? Self.presets.map { preset in
                FundingAmountViewModel.Preset(text: preset.stringWithPointSeparator, figure: cashFigure(preset))
            } : [],
            issue: state.amountIssue.map(issueText),
            minimum: minimumCaption,
            canContinue: state.canContinue
        )
    }

    func makeSummaryViewModel(for state: FundingEntryModel) -> FundingSummaryViewModel {
        let quote = state.selectedQuote
        let payout = quote.flatMap { quote in
            state.direction == .in
                ? cashLabel(units: quote.receiveAmount)
                : quoteAmount(quote.receiveAmount, of: quote, in: state)
        }

        return FundingSummaryViewModel(
            title: summaryTitle(for: state),
            feesCaption: summaryFeesCaption(for: state),
            showsCountry: state.rail != .crypto,
            country: state.country,
            provider: state.selectedProviderId.map(state.brand(for:)),
            headline: quote.flatMap { quote in
                quoteAmount(state.direction == .in ? quote.sendAmount : quote.receiveAmount, of: quote, in: state)
            },
            isQuoting: state.isQuoting,
            payoutTitle: state.direction == .in
                ? String(localized: .Funding.summaryMinPayout)
                : String(localized: .Funding.summaryYouReceive),
            payout: payout,
            eta: quote?.etaSecs.flatMap(etaText(seconds:)),
            bankRateNote: state.rail == .bank && state.direction == .in ? bankRateNote : nil,
            problem: summaryProblem(for: state),
            hasQuote: quote != nil,
            isStarting: state.isStarting
        )
    }

    func makeFeesViewModel(for state: FundingEntryModel) -> FundingFeesViewModel {
        let payTitle =
            switch (state.direction, state.rail) {
            case (.in, .card): String(localized: .Funding.feesYouPay)
            case (.in, _): String(localized: .Funding.feesAmountToSend)
            case (.out, _): String(localized: .Funding.feesYouSend)
            }

        return FundingFeesViewModel(payTitle: payTitle, breakdown: feesBreakdown(for: state))
    }

    func makeProvidersViewModel(for state: FundingEntryModel) -> FundingProvidersViewModel {
        let quotedCount = state.quotedProviders.count
        let rows = state.listedProviderIds.map { providerId in
            let quoteState = state.rows[providerId] ?? .pending
            let badge: FundingProvidersViewModel.Row.Badge? =
                if case let .unavailable(reason) = quoteState {
                    .unavailable(unavailableText(reason))
                } else if providerId == state.bestProviderId, quotedCount > 1 {
                    .lowestPrice
                } else {
                    nil
                }

            return FundingProvidersViewModel.Row(
                providerId: providerId,
                brand: state.brand(for: providerId),
                isSelected: providerId == state.selectedProviderId,
                badge: badge,
                price: providerPrice(quoteState, in: state)
            )
        }

        return FundingProvidersViewModel(
            rows: rows,
            isRefreshing: state.rows.values.contains(where: \.isPending),
            refreshAt: state.quotesRefreshAt
        )
    }

    func makeNetworkViewModel(for state: FundingEntryModel) -> FundingNetworkViewModel {
        let amount = state.amount ?? 0
        let rows = state.networks.map { network in
            let minimum = state.minimum(network: network.id).flatMap { minimum in
                minimum > amount ? String(localized: .Funding.networkMinimum(amount: cashLabel(minimum))) : nil
            }
            return FundingNetworkViewModel.Row(mark: mark(network, in: state), minimum: minimum)
        }
        return FundingNetworkViewModel(rows: rows)
    }

    func makeTokenViewModel(for state: FundingEntryModel) -> FundingTokenViewModel {
        FundingTokenViewModel(
            network: state.network.map { mark(FundingNetwork(id: $0), in: state) },
            tokens: state.tokens.map { token in
                FundingAssetMarkViewModel(
                    id: token.symbol,
                    name: token.symbol,
                    monogram: token.monogram,
                    icon: state.icon(token: token.symbol)
                )
            }
        )
    }

    func makeCountryViewModel(for state: FundingEntryModel) -> FundingCountryViewModel {
        let codes = (state.entry.options?.countries ?? []).map { $0.country.uppercased() }
        let minimums = codes.reduce(into: [String: String]()) { minimums, code in
            guard let minimum = state.minimum(country: code) else { return }
            minimums[code] = String(localized: .Funding.countryMinimum(amount: cashLabel(minimum)))
        }
        return FundingCountryViewModel(
            detected: state.detectedCountry,
            selected: state.country,
            unsupported: state.unsupportedCountries,
            minimums: minimums
        )
    }

    func makeCancelConfirmViewModel(for state: FundingEntryModel) -> FundingCancelConfirmViewModel {
        let message: LocalizedStringResource =
            if !state.isUnpaidTopUp {
                .Funding.cancelBodyProvider
            } else {
                state.rail == .bank ? .Funding.cancelBodyBank : .Funding.cancelBody
            }

        return FundingCancelConfirmViewModel(
            title: String(localized: state.direction == .in ? .Funding.cancelTitle : .Funding.cancelTitleWithdraw),
            message: String(localized: message),
            isCancelling: state.isCancelling
        )
    }

    func makeEndedViewModel(for ended: FundingEndedSession) -> FundingEndedViewModel {
        let refunded = Balance(units: ended.summary.refunded).map(cashLabel)

        return FundingEndedViewModel(
            figure: ended.cashAmount.map { amount in
                ended.succeeded ? signedCashFigure(amount, direction: ended.direction) : cashFigure(amount)
            },
            symbol: cashSymbol,
            quoted: ended.quotedAmount.map { quoted in
                assetAmount(quoted.amount, info: .from(assetCode: quoted.asset, decimals: Int16(quoted.decimals)))
            },
            settled: ended.settledAmount.map(cashLabel),
            requested: ended.differingRequestedAmount.map(cashLabel),
            refunded: refunded,
            amount: ended.cashAmount.map(cashLabel) ?? "$\(cashSymbol)",
            endedAt: timestampFormatter.string(for: ended.endedAt),
            eta: ended.quote?.etaSecs.flatMap(etaText(seconds:))
        )
    }

    func makeActivityRow(for item: FundingActivityItem, now: Date) -> FundingActivityRowContent {
        FundingActivityRowContent(
            item: item,
            title: activityTitle(for: item),
            moment: timestampFormatter.string(for: item.date, now: now)
        )
    }

    /// "About 5 min", as the provider gave it in seconds.
    func etaText(seconds: UInt64) -> String? {
        try? etaFormatter.string(from: TimeInterval(seconds))
    }

    /// Unsigned in flight and once nothing moved, signed once value moved.
    func activityTitle(for item: FundingActivityItem) -> String {
        guard let amount = item.amount else { return "" }

        switch item.status {
        case .toppedUp,
             .sent:
            return signedCashFigure(amount, direction: item.direction) + " \(cashSymbol)"
        case .inProgress where item.direction == .out:
            return signedCashFigure(amount, direction: .out) + " \(cashSymbol)"
        case .inProgress,
             .refunded,
             .payoutFailed,
             .cancelled,
             .failed:
            return cashLabel(amount)
        }
    }
}

private extension FundingViewModelFactory {
    func summaryTitle(for state: FundingEntryModel) -> String {
        switch (state.direction, state.rail) {
        case (.in, .card): String(localized: .Funding.summaryCardTitle)
        case (.in, .bank): String(localized: .Funding.summaryBankTitle)
        case (.in, .crypto): String(localized: .Funding.summaryCryptoTitle)
        case (.out, .card): String(localized: .Funding.summaryWithdrawCardTitle)
        case (.out, .bank): String(localized: .Funding.summaryWithdrawBankTitle)
        case (.out, .crypto): String(localized: .Funding.summaryWithdrawCryptoTitle)
        }
    }

    func summaryFeesCaption(for state: FundingEntryModel) -> String {
        switch (state.direction, state.rail) {
        case (.in, .card): String(localized: .Funding.summaryCardCaption)
        case (.in, _): String(localized: .Funding.summaryBankCaption)
        case (.out, _): String(localized: .Funding.summaryWithdrawCaption)
        }
    }

    func summaryProblem(for state: FundingEntryModel) -> String? {
        if let failure = state.failure {
            return failure.failedText
        }
        if let startError = state.startError {
            return startError
        }
        guard state.noProviderQuoted else { return nil }

        switch state.unavailableIssue {
        case let .some(issue) where issue != .notEnoughBalance:
            return issueText(issue)
        case .some,
             nil:
            return state.quoteFailureText ?? String(localized: .Funding.errorNoProvider)
        }
    }

    func feesBreakdown(for state: FundingEntryModel) -> FundingFeesViewModel.Breakdown? {
        guard let quote = state.chosenQuote else { return nil }

        let fees = FundingFees(quote: quote, direction: state.direction, cashPrecision: cashInfo.assetPrecision)
        // Fees are counted in what the user pays: the ask's asset for value
        // in, the balance for value out.
        let feeText: (Balance?) -> String? = { [self] amount in
            amount.map { state.direction == .in ? assetAmount($0, info: state.quoteUnit(quote)) : cashLabel($0) }
        }

        return FundingFeesViewModel.Breakdown(
            amount: feeText(fees.net),
            providerFee: feeText(Balance(units: quote.providerFee)),
            networkFee: feeText(Balance(units: quote.networkFee)),
            total: feeText(fees.total),
            pay: feeText(Balance(units: quote.sendAmount)),
            rate: rate(fees, of: quote, in: state)
        )
    }

    /// What one CASH is worth in the provider-side asset, net of fees as the
    /// amount line above it.
    func rate(_ fees: FundingFees, of quote: FundingQuote, in state: FundingEntryModel) -> String? {
        let info = state.quoteUnit(quote)
        let providerSide = fees.rateNum.decimal(assetInfo: info)
        let cashSide = fees.rateDen.decimal(assetInfo: cashInfo)
        guard cashSide > 0 else { return nil }

        let perCash = assetAmount((providerSide / cashSide).rounded(scale: 4), info: info)
        let oneCash = "\(cashFigure(Decimal(1))) \(cashSymbol)"
        return String(localized: .Funding.feesRateValue(cash: oneCash, value: perCash))
    }

    func providerPrice(
        _ quoteState: FundingQuoteState,
        in state: FundingEntryModel
    ) -> FundingProvidersViewModel.Row.Price {
        switch quoteState {
        case let .quoted(quote):
            let providerSide = state.direction == .in ? quote.sendAmount : quote.receiveAmount
            let cashSide = state.direction == .in ? quote.receiveAmount : quote.sendAmount
            return .quoted(
                provider: quoteAmount(providerSide, of: quote, in: state).map { "≈ " + $0 },
                cash: cashLabel(units: cashSide).map { String(localized: .Funding.providersFor(amount: $0)) }
            )
        case .unavailable:
            return .unavailable
        case .pending:
            return .pending
        }
    }

    func unavailableText(_ reason: FundingQuoteUnavailable) -> String {
        switch reason {
        case let .refused(.belowMinimum(min)):
            cashLabel(units: min).map { String(localized: .Funding.errorMinimum(amount: $0)) }
                ?? String(localized: .Funding.providersUnavailable)
        case let .refused(.aboveMaximum(max)):
            cashLabel(units: max).map { String(localized: .Funding.errorMaximum(amount: $0)) }
                ?? String(localized: .Funding.providersUnavailable)
        case .refused(.countryUnsupported):
            String(localized: .Funding.providersCountryUnsupported)
        case .timeout,
             .refused:
            String(localized: .Funding.providersUnavailable)
        }
    }

    func mark(_ network: FundingNetwork, in state: FundingEntryModel) -> FundingAssetMarkViewModel {
        FundingAssetMarkViewModel(
            id: network.id,
            name: network.name,
            monogram: network.monogram,
            icon: state.icon(network: network.id)
        )
    }
}

private extension Decimal {
    func rounded(scale: Int) -> Decimal {
        var source = self
        var result = Decimal()
        NSDecimalRound(&result, &source, scale, .plain)
        return result
    }

    /// The amount as the keypad types it.
    var stringWithPointSeparator: String {
        NSDecimalNumber(decimal: self).stringValue
    }
}
