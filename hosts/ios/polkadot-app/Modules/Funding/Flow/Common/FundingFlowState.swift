import Foundation
import TrUAPIHost

/// What the user picked so far in one funding flow, shared by its screens.
struct FundingDraft: Equatable {
    var rail: FundingRail
    var amountText: String
    /// Where the user pays from: the device's region until the user picks one.
    var country: PaymentCountry?
    /// The country the core detected, which the country list leads with.
    var detectedCountry: PaymentCountry?
    var hasPickedCountry = false
    var network: String?
    var asset: String?
    var chosenProviderId: ProductId?
    /// What a payment can spend now, once the app has read it.
    var spendable: Decimal?

    /// The selection the core is told of for the draft.
    var selection: FundingSelection {
        FundingSelection(
            rail: rail,
            asset: rail == .crypto ? asset : nil,
            network: rail == .crypto ? network : nil,
            country: country?.code,
            currency: country?.currencyCode
        )
    }
}

/// What one funding screen draws: the session the core holds, what the user
/// picked, and what this screen is doing.
struct FundingFlowState {
    var entry: FundingEntry
    var draft: FundingDraft
    let cash: FundingCash
    let branding: FundingProviderBranding
    var brands: [ProductId: FundingProviderBrand] = [:]
    /// Why the typed amount cannot go ahead, as the core decides it.
    var amountIssue: FundingAmountIssue?
    var isStarting = false
    /// Why the chosen provider could not be handed the session.
    var startError: String?
    var isCancelling = false
    var isAnsweringOffer = false
}

// MARK: - Draft

extension FundingFlowState {
    var direction: FundingDirection { entry.direction }
    var rail: FundingRail { entry.session?.choice?.rail ?? draft.rail }
    var amountText: String { draft.amountText }
    var country: PaymentCountry? { draft.country }
    var detectedCountry: PaymentCountry? { draft.detectedCountry }
    var network: String? { draft.network }
    var asset: String? { entry.session?.choice?.asset ?? draft.asset }
    var spendable: Decimal? { draft.spendable }

    /// The typed amount in CASH units: nil while the text is not a number.
    var amountUnits: U128? {
        cash.units(text: draft.amountText)
    }

    var amount: Decimal {
        cash.decimal(amountUnits) ?? 0
    }

    func brand(for providerId: ProductId) -> FundingProviderBrand {
        brands[providerId] ?? .placeholder(for: providerId)
    }
}

// MARK: - Session

extension FundingFlowState {
    var session: FundingSession? { entry.session }
    var progress: FundingProgress? { entry.progress }
    var failure: FundingFailure? { entry.failure }
    var ended: FundingEndedSession? { entry.ended }
    var hasStarted: Bool { entry.hasStarted }
    var isUnpaidTopUp: Bool { entry.isUnpaidTopUp(rail: draft.rail) }
    var cancelOffer: (fee: U128, refund: U128)? { entry.cancelOffer }
    var canRequestCancel: Bool { entry.canRequestCancel }
    var cancelTitle: String { entry.cancelTitle }

    func hasReached(_ step: FundingStep) -> Bool {
        entry.hasReached(step)
    }

    /// The quote the session was started on, or the one on screen before it.
    var chosenQuote: FundingQuote? {
        session?.choice?.quote ?? selectedQuote
    }

    /// The asset `quote`'s provider-side figures are counted in, with the
    /// decimals the quote carries.
    func quoteUnit(_ quote: FundingQuote) -> FundingAssetUnit {
        FundingAssetUnit(code: session?.choice?.asset ?? askedAsset ?? "", decimals: quote.assetDecimals)
    }

    /// The asset an ask names: the chosen token for crypto, and for card or
    /// bank the one the core offers for the payment country's currency.
    var askedAsset: String? {
        draft.rail == .crypto ? draft.asset : entry.options?.asset
    }
}

// MARK: - Options

extension FundingFlowState {
    /// The rails the core says a provider routes.
    var availableRails: Set<FundingRail> {
        Set(entry.options?.rails ?? [])
    }

    var canContinue: Bool {
        amount > 0 && amountIssue == nil && availableRails.contains(draft.rail)
    }

    /// The range the core says the reported selection takes across every
    /// provider, product floor included.
    var amountLimits: (min: Decimal, max: Decimal?) {
        let limits = entry.options?.limits
        return (min: cash.decimal(limits?.min) ?? 0, max: cash.decimal(limits?.max))
    }

    /// The networks the core says a crypto route takes, named as it names them.
    var networks: [FundingNetwork] {
        (entry.options?.networks ?? []).map { FundingNetwork(id: $0.network) }
    }

    /// The smallest amount the core says this network takes for the
    /// reported selection.
    func minimum(network id: String) -> Decimal? {
        cash.decimal(entry.options?.networks.first { $0.network == id }?.limits.min)
    }

    /// The tokens the core says a session can use on the picked network.
    var tokens: [FundingToken] {
        (entry.options?.tokens ?? []).map(FundingToken.init(symbol:))
    }

    /// Countries the core says no provider takes payment from on this rail.
    var unsupportedCountries: Set<String> {
        Set((entry.options?.countries ?? []).compactMap { country in
            guard case .unsupported = country.support else { return nil }
            return country.country.uppercased()
        })
    }

    /// The smallest amount a provider showed it takes in `code`, when one did.
    func minimum(country code: String) -> Decimal? {
        let support = entry.options?.countries.last { $0.country.uppercased() == code.uppercased() }?.support
        guard case let .supported(min)? = support, let min else { return nil }
        return cash.decimal(min)
    }
}

// MARK: - Quotes

extension FundingFlowState {
    var rows: [ProductId: FundingQuoteState] {
        Dictionary((entry.quotes?.rows ?? []).map { ($0.providerId, $0.state) }) { _, latest in latest }
    }

    /// The providers of the core's latest quote list, in its order: the core
    /// asks only those routing the rail.
    var listedProviderIds: [ProductId] {
        entry.quotes?.rows.map(\.providerId) ?? []
    }

    /// The provider the core marked as quoting best for the user.
    var bestProviderId: ProductId? {
        entry.quotes?.best
    }

    /// When the core asks every provider again, and the prices on screen change.
    var quotesRefreshAt: Date? {
        entry.quotes.map { Date(milliseconds: $0.refreshAtMs) }
    }

    var quotedProviders: [(providerId: ProductId, quote: FundingQuote)] {
        rows.compactMap { providerId, state in state.quote.map { (providerId, $0) } }
    }

    var selectedProviderId: ProductId? {
        if let chosen = draft.chosenProviderId, rows[chosen]?.quote != nil { return chosen }
        return bestProviderId
    }

    var selectedQuote: FundingQuote? {
        selectedProviderId.flatMap { rows[$0]?.quote }
    }

    var isQuoting: Bool {
        entry.isQuoting || (rows.values.contains(where: \.isPending) && selectedQuote == nil)
    }

    /// What every provider said about the amount when none would quote.
    var unavailableIssue: FundingAmountIssue? {
        noProviderQuoted ? entry.quotes?.issue : nil
    }

    var noProviderQuoted: Bool {
        !entry.isQuoting && !rows.isEmpty && quotedProviders.isEmpty && !rows.values.contains(where: \.isPending)
    }

    /// Why no provider quoted, when the core says they all gave the same
    /// reason: no answer in time, the payment country, or one of their own.
    var quoteFailureText: String? {
        switch entry.quotes?.refusal {
        case .timeout: String(localized: .Funding.errorQuoteTimeout)
        case .refused(.countryUnsupported): String(localized: .Funding.providersCountryUnsupported)
        case let .refused(.other(message)): message
        case .refused,
             nil: nil
        }
    }
}
