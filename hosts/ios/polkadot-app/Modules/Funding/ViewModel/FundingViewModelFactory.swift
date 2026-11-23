import Foundation
import Foundation_iOS
import FoundationExt
import SubstrateSdk
import TrUAPIHost

/// Turns what the funding screens draw into their text. CASH is set as the
/// app sets the digital dollar figure, with its cents only when there are
/// any, followed by the payment asset's symbol; a provider's asset as the app
/// sets a price for a currency and a token balance for anything else.
protocol FundingViewModelFactoryProtocol {
    func makeAmountViewModel(for state: FundingFlowState) -> FundingAmountViewModel
    func makeSummaryViewModel(for state: FundingFlowState) -> FundingSummaryViewModel
    func makeFeesViewModel(for state: FundingFlowState) -> FundingFeesViewModel?
    func makeProvidersViewModel(for state: FundingFlowState) -> FundingProvidersViewModel
    func makeNetworkViewModel(for state: FundingFlowState) -> FundingNetworkViewModel
    func makeCountryViewModel(for state: FundingFlowState) -> FundingCountryViewModel
    func makeDepositViewModel(for state: FundingFlowState) -> FundingDepositViewModel
    func makeProgressViewModel(for state: FundingFlowState) -> FundingProgressViewModel
    func makeEndedViewModel(for ended: FundingEndedSession) -> FundingEndedViewModel
    func makeActivityTitle(for item: FundingActivityItem) -> String
}

final class FundingViewModelFactory {
    /// The top-up amounts offered under the figure, in dollars.
    private static let presets: [Decimal] = [10, 50, 100]

    let cashInfo: AssetBalanceDisplayInfo
    let cashSymbol: String
    let formatterFactory: AssetBalanceFormatterFactoryProtocol

    private let wholeCashFormatter: LocalizableDecimalFormatting
    private let centsCashFormatter: LocalizableDecimalFormatting

    init(
        cashInfo: AssetBalanceDisplayInfo,
        cashSymbol: String,
        formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory()
    ) {
        self.cashInfo = cashInfo
        self.cashSymbol = cashSymbol
        self.formatterFactory = formatterFactory

        wholeCashFormatter = formatterFactory
            .createAssetPriceFormatter(for: cashInfo, minimumFractionDigits: 0)
            .value(for: .current)
        centsCashFormatter = formatterFactory
            .createAssetPriceFormatter(for: cashInfo, minimumFractionDigits: cashInfo.displayPrecision)
            .value(for: .current)
    }
}

// MARK: - Amounts

private extension FundingViewModelFactory {
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
    func quoteAmount(_ units: U128, of quote: FundingQuote, in state: FundingFlowState) -> String? {
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
    func makeAmountViewModel(for state: FundingFlowState) -> FundingAmountViewModel {
        let minimum = cashLabel(state.amountLimits.min)
        let minimumCaption = state.direction == .in
            ? String(localized: .Funding.amountMinimum(amount: minimum))
            : String(localized: .Funding.amountWithdrawMinimum(amount: minimum))

        return FundingAmountViewModel(
            available: state.direction == .out ? state.spendable.map(cashLabel) : nil,
            figure: cashFigure(state.amount ?? 0),
            symbol: cashSymbol,
            presets: state.direction == .in ? Self.presets.map { preset in
                FundingAmountViewModel.Preset(text: preset.stringWithPointSeparator, figure: cashFigure(preset))
            } : [],
            issue: state.amountIssue.map(issueText),
            minimum: minimumCaption
        )
    }

    func makeSummaryViewModel(for state: FundingFlowState) -> FundingSummaryViewModel {
        let quote = state.selectedQuote
        let payout = quote.flatMap { quote in
            state.direction == .in
                ? cashLabel(units: quote.receiveAmount)
                : quoteAmount(quote.receiveAmount, of: quote, in: state)
        }

        return FundingSummaryViewModel(
            headline: quote.flatMap { quote in
                quoteAmount(state.direction == .in ? quote.sendAmount : quote.receiveAmount, of: quote, in: state)
            },
            payout: payout,
            bankRateNote: bankRateNote,
            problem: summaryProblem(for: state)
        )
    }

    func makeFeesViewModel(for state: FundingFlowState) -> FundingFeesViewModel? {
        guard let quote = state.chosenQuote else { return nil }

        let fees = FundingFees(quote: quote, direction: state.direction, cashPrecision: cashInfo.assetPrecision)
        // Fees are counted in what the user pays: the ask's asset for value
        // in, the balance for value out.
        let feeText: (Balance?) -> String? = { [self] amount in
            amount.map { state.direction == .in ? assetAmount($0, info: state.quoteUnit(quote)) : cashLabel($0) }
        }

        return FundingFeesViewModel(
            amount: feeText(fees.net),
            providerFee: feeText(Balance(units: quote.providerFee)),
            networkFee: feeText(Balance(units: quote.networkFee)),
            total: feeText(fees.total),
            pay: feeText(Balance(units: quote.sendAmount)),
            rate: rate(fees, of: quote, in: state)
        )
    }

    func makeProvidersViewModel(for state: FundingFlowState) -> FundingProvidersViewModel {
        var prices: [ProductId: FundingProvidersViewModel.Price] = [:]
        var unavailable: [ProductId: String] = [:]

        for (providerId, quoteState) in state.rows {
            switch quoteState {
            case let .quoted(quote):
                let providerSide = state.direction == .in ? quote.sendAmount : quote.receiveAmount
                let cashSide = state.direction == .in ? quote.receiveAmount : quote.sendAmount
                prices[providerId] = FundingProvidersViewModel.Price(
                    provider: quoteAmount(providerSide, of: quote, in: state).map { "≈ " + $0 },
                    cash: cashLabel(units: cashSide).map { String(localized: .Funding.providersFor(amount: $0)) }
                )
            case let .unavailable(reason):
                unavailable[providerId] = unavailableText(reason)
            case .pending:
                break
            }
        }

        return FundingProvidersViewModel(prices: prices, unavailable: unavailable)
    }

    func makeNetworkViewModel(for state: FundingFlowState) -> FundingNetworkViewModel {
        let amount = state.amount ?? 0
        let minimums = state.networks.reduce(into: [String: String]()) { minimums, network in
            guard let minimum = state.minimum(network: network.id), minimum > amount else { return }
            minimums[network.id] = String(localized: .Funding.networkMinimum(amount: cashLabel(minimum)))
        }
        return FundingNetworkViewModel(minimums: minimums)
    }

    func makeCountryViewModel(for state: FundingFlowState) -> FundingCountryViewModel {
        let codes = (state.entry.options?.countries ?? []).map { $0.country.uppercased() }
        let minimums = codes.reduce(into: [String: String]()) { minimums, code in
            guard let minimum = state.minimum(country: code) else { return }
            minimums[code] = String(localized: .Funding.countryMinimum(amount: cashLabel(minimum)))
        }
        return FundingCountryViewModel(minimums: minimums)
    }

    func makeDepositViewModel(for state: FundingFlowState) -> FundingDepositViewModel {
        switch state.progress?.deposit {
        case let .crypto(_, _, asset, amount, decimals, _, _, _):
            FundingDepositViewModel(amount: assetAmount(units: amount, asset: asset, decimals: decimals))
        case let .bank(amount, currency, decimals, _, _, _, _, _):
            FundingDepositViewModel(amount: assetAmount(units: amount, asset: currency, decimals: decimals))
        case nil:
            FundingDepositViewModel(amount: nil)
        }
    }

    func makeProgressViewModel(for state: FundingFlowState) -> FundingProgressViewModel {
        let quote = state.chosenQuote
        let cashAmount = state.session
            .flatMap { FundingActivityItem(session: $0, progress: state.progress).amount }
            ?? quote.flatMap { Balance(units: state.direction == .in ? $0.receiveAmount : $0.sendAmount) }

        return FundingProgressViewModel(
            figure: cashAmount.map(cashFigure),
            symbol: cashSymbol,
            amount: cashAmount.map(cashLabel),
            quotedSend: quote.flatMap { quoteAmount($0.sendAmount, of: $0, in: state) },
            quotedReceive: quote.flatMap { quoteAmount($0.receiveAmount, of: $0, in: state) },
            deposit: makeDepositViewModel(for: state).amount,
            bankRateNote: bankRateNote,
            cancelOffer: state.cancelOffer.map { offer in
                FundingProgressViewModel.CancelOffer(fee: cashLabel(offer.fee), refund: cashLabel(offer.refund))
            },
            ended: state.ended.map(makeEndedViewModel)
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
            amount: ended.cashAmount.map(cashLabel) ?? "$\(cashSymbol)"
        )
    }

    /// Unsigned in flight and once nothing moved, signed once value moved.
    func makeActivityTitle(for item: FundingActivityItem) -> String {
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
    func summaryProblem(for state: FundingFlowState) -> String? {
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

    /// What one CASH is worth in the provider-side asset, net of fees as the
    /// amount line above it.
    func rate(_ fees: FundingFees, of quote: FundingQuote, in state: FundingFlowState) -> String? {
        let info = state.quoteUnit(quote)
        let providerSide = fees.rateNum.decimal(assetInfo: info)
        let cashSide = fees.rateDen.decimal(assetInfo: cashInfo)
        guard cashSide > 0 else { return nil }

        let perCash = assetAmount((providerSide / cashSide).rounded(scale: 4), info: info)
        let oneCash = "\(cashFigure(Decimal(1))) \(cashSymbol)"
        return String(localized: .Funding.feesRateValue(cash: oneCash, value: perCash))
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
