import Foundation
import TrUAPIHost

extension FundingFlowModel {
    func brand(for providerId: ProductId) -> FundingProviderBrand {
        brands.brand(for: providerId)
    }

    /// What the providers are asked to quote for the selection on screen.
    func currentAsk() -> FundingQuoteAsk? {
        guard let amountUnits, amount > 0, let askedAsset else { return nil }

        return FundingQuoteAsk(
            direction: direction,
            rail: rail,
            asset: askedAsset,
            network: rail == .crypto ? network : nil,
            amount: amountUnits,
            country: country?.code
        )
    }

    /// The asset an ask names: the chosen token for crypto, and for card or
    /// bank the one the core offers for the payment country's currency.
    var askedAsset: String? {
        rail == .crypto ? asset : offeredAsset
    }

    /// The typed amount in CASH units, as the core reads it: nil while the
    /// text is not a number.
    var amountUnits: U128? {
        fundingUnits(text: amountText, decimals: UInt8(clamping: cash.precision))
    }

    var amount: Decimal {
        cash.decimal(amountUnits) ?? 0
    }

    /// The rails the core says a provider routes.
    var availableRails: Set<FundingRail> {
        Set(rails)
    }

    /// The providers of the core's latest quote list, in its order: the core
    /// asks only those routing the rail.
    var listedProviderIds: [ProductId] {
        quoteOrder
    }

    var canContinue: Bool {
        amount > 0 && amountIssue == nil && availableRails.contains(rail)
    }

    /// The range the core says the reported selection takes across every
    /// provider, product floor included.
    var amountLimits: (min: Decimal, max: Decimal?) {
        (min: cash.decimal(limits.min) ?? 0, max: cash.decimal(limits.max))
    }

    /// Why the typed amount cannot go ahead, as the core decides it from the
    /// limits, the latest quote list and what a withdrawal can spend.
    var amountIssue: FundingAmountIssue? {
        guard let amountUnits else { return nil }

        return fundingAmountIssue(
            direction: direction,
            limits: limits,
            quotes: quotes.list,
            amount: amountUnits,
            spendable: spendable.flatMap { cash.units($0) }
        )
    }

    /// The asset `quote`'s provider-side figures are counted in, with the
    /// decimals the quote carries.
    func quoteUnit(_ quote: FundingQuote) -> FundingAssetUnit {
        FundingAssetUnit(code: ask?.asset ?? session?.choice?.asset ?? askedAsset ?? "", decimals: quote.assetDecimals)
    }
}

// MARK: - Session

extension FundingFlowModel {
    /// Why the session ended, once it has failed.
    var failure: FundingFailure? {
        guard case let .failed(reason, _) = session?.stage else { return nil }
        return reason
    }

    /// The session once it has ended, as its ended screen draws it.
    var ended: FundingEndedSession? {
        summary.flatMap { FundingEndedSession(summary: $0, session: session, progress: progress) }
    }

    /// The quote the session was started on, or the one on screen before it.
    var chosenQuote: FundingQuote? {
        session?.choice?.quote ?? selectedQuote
    }

    /// Whether the session got to `step`.
    func hasReached(_ step: FundingStep) -> Bool {
        progress?.steps.contains { $0.step == step && $0.reachedAtMs != nil } ?? false
    }

    /// A bank or crypto top-up whose payment the provider has not seen:
    /// cancelling ends it outright, and money sent anyway still arrives.
    var isUnpaidTopUp: Bool {
        direction == .in && rail != .card && !hasReached(.received)
    }

    /// The provider's terms for cancelling, while it waits for the user.
    var cancelOffer: (fee: U128, refund: U128)? {
        guard case let .cancelOffered(fee, refund) = progress?.status else { return nil }
        return (fee, refund)
    }

    /// Whether the core lets the user cancel the started session now.
    var canRequestCancel: Bool {
        hasStarted && (progress?.cancel ?? .unavailable) != .unavailable
    }

    /// The cancel action's title for what the core offers.
    var cancelTitle: String {
        progress?.cancel.title ?? String(localized: .Funding.depositCancel)
    }
}

// MARK: - Quotes

extension FundingFlowModel {
    var rows: [ProductId: FundingQuoteState] {
        quotes.rows
    }

    /// The providers of the core's latest quote list, in its order.
    var quoteOrder: [ProductId] {
        quotes.order
    }

    /// The provider the core marked as quoting best for the user.
    var bestProviderId: ProductId? {
        quotes.bestProviderId
    }

    /// What every provider was asked last.
    var ask: FundingQuoteAsk? {
        quotes.ask
    }

    /// When the core asks every provider again, and the prices on screen change.
    var quotesRefreshAt: Date? {
        quotes.refreshAt
    }

    var quotedProviders: [(providerId: ProductId, quote: FundingQuote)] {
        rows.compactMap { providerId, state in state.quote.map { (providerId, $0) } }
    }

    var selectedProviderId: ProductId? {
        if let chosenProviderId, rows[chosenProviderId]?.quote != nil { return chosenProviderId }
        return bestProviderId
    }

    var selectedQuote: FundingQuote? {
        selectedProviderId.flatMap { rows[$0]?.quote }
    }

    var isQuoting: Bool {
        quotes.isAwaiting || (rows.values.contains(where: \.isPending) && selectedQuote == nil)
    }

    /// What every provider said about the amount when none would quote, for
    /// the summary screen.
    var unavailableIssue: FundingAmountIssue? {
        noProviderQuoted ? quotes.list?.issue : nil
    }

    var noProviderQuoted: Bool {
        !quotes.isAwaiting && !rows.isEmpty && quotedProviders.isEmpty && !rows.values.contains(where: \.isPending)
    }

    /// Why no provider quoted, when the core says they all gave the same
    /// reason: no answer in time, the payment country, or one of their own.
    var quoteFailureText: String? {
        switch quotes.list?.refusal {
        case .timeout: String(localized: .Funding.errorQuoteTimeout)
        case .countryUnsupported: String(localized: .Funding.providersCountryUnsupported)
        case let .other(message): message
        case nil: nil
        }
    }
}

// MARK: - Crypto and countries

extension FundingFlowModel {
    /// The networks the core says a crypto route takes, named as it names them.
    var networks: [FundingNetwork] {
        networkLimits.map { FundingNetwork(id: $0.network) }
    }

    /// The smallest amount the core says this network takes for the
    /// reported selection.
    func minimum(network id: String) -> Decimal? {
        cash.decimal(networkLimits.first { $0.network == id }?.limits.min)
    }

    /// The tokens the core says a session can use on the picked network.
    var tokens: [FundingToken] {
        tokenSymbols.map(FundingToken.init(symbol:))
    }

    /// Countries the core says no provider takes payment from on this rail.
    var unsupportedCountries: Set<String> {
        Set(countrySupport.compactMap { code, support in
            guard case .unsupported = support else { return nil }
            return code
        })
    }

    /// The smallest amount a provider showed it takes in `code`, when one did.
    func minimum(country code: String) -> Decimal? {
        guard case let .supported(min) = countrySupport[code.uppercased()], let min else { return nil }
        return cash.decimal(min)
    }
}
