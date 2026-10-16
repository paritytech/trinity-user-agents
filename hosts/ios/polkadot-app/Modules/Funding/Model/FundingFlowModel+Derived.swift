import Foundation
import TrUAPIHost

/// Why the typed amount cannot go ahead.
enum FundingAmountIssue: Equatable {
    case belowMinimum(Decimal)
    case aboveMaximum(Decimal)
    case notEnoughBalance
}

extension FundingFlowModel {
    /// What the core's amount range depends on.
    struct FundingLimitsKey: Equatable {
        let rail: FundingRail
        let asset: String
        let network: String?
        let country: String?
    }

    func currentAsk() -> FundingQuoteAsk? {
        guard amount > 0 else { return nil }

        let askedAsset: String
        switch rail {
        case .crypto:
            guard let asset else { return nil }
            askedAsset = asset
        case .card,
             .bank:
            askedAsset = fiatAsset
        }

        return FundingQuoteAsk(
            direction: direction,
            rail: rail,
            asset: askedAsset,
            network: rail == .crypto ? network : nil,
            amount: cash.units(amount),
            country: country?.code
        )
    }

    var amount: Decimal {
        Decimal(string: amountText, locale: Locale(identifier: "en_US")) ?? 0
    }

    /// Every funding provider is asked on every rail, so card and bank are
    /// offered whenever one exists and the quotes decide. Crypto also needs a
    /// network and a token, which only routes list.
    var availableRails: Set<FundingRail> {
        guard !candidates.isEmpty else { return [] }
        return Set(FundingRail.displayOrder.filter { $0 != .crypto || !cryptoRoutes.isEmpty })
    }

    /// The providers declaring a route for this rail. Routes are hints: they
    /// order the list and suggest the asset, they never leave a provider out.
    var routedCandidates: [FundingCandidate] {
        candidates.filter { $0.serves(rail: rail, direction: direction) }
    }

    /// The providers the list shows: the core's quote list in its order once it
    /// arrived, otherwise every provider with the routed ones first.
    var listedProviderIds: [String] {
        if !quoteOrder.isEmpty { return quoteOrder }
        let routed = Set(routedCandidates.map(\.providerId))
        return candidates.filter { routed.contains($0.providerId) }.map(\.providerId)
            + candidates.filter { !routed.contains($0.providerId) }.map(\.providerId)
    }

    var canContinue: Bool {
        amount > 0 && amountIssue == nil && availableRails.contains(rail)
    }

    /// The range the core says this rail, asset, network and country take
    /// across every provider, product floor included. Read once per choice
    /// and again after each quote list, which can teach the core new limits.
    var amountLimits: (min: Decimal, max: Decimal?) {
        let key = FundingLimitsKey(
            rail: rail,
            asset: askedAsset ?? "",
            network: rail == .crypto ? network : nil,
            country: country?.code
        )
        if let cached = cachedLimits, cached.key == key { return cached.limits }

        let limits = runtime.fundingAmountLimits(
            intent: intent,
            rail: key.rail,
            asset: key.asset,
            network: key.network,
            country: key.country
        )
        let range = (min: cash.decimal(limits.min) ?? 0, max: cash.decimal(limits.max))
        cachedLimits = (key, range)
        return range
    }

    /// The asset an ask on this rail names: the chosen token for crypto,
    /// the payment currency otherwise.
    var askedAsset: String? {
        ask?.asset ?? (rail == .crypto ? asset : fiatAsset)
    }

    var amountIssue: FundingAmountIssue? {
        if direction == .out, let spendable, amount > spendable {
            return .notEnoughBalance
        }

        if let refused = refusalForCurrentAmount {
            return refused
        }

        let limits = amountLimits
        if amount > 0, amount < limits.min {
            return .belowMinimum(limits.min)
        }
        if let maximum = limits.max, amount > maximum {
            return .aboveMaximum(maximum)
        }
        return nil
    }

    /// Every provider asked about this exact amount refused it on a limit.
    var refusalForCurrentAmount: FundingAmountIssue? {
        guard let ask, ask.amount == cash.units(amount), !rows.isEmpty else { return nil }
        guard !rows.values.contains(where: { $0.isPending || $0.quote != nil }) else { return nil }

        let refusals = rows.values.compactMap(\.refusal)
        let minimums = refusals.compactMap { refusal -> Decimal? in
            guard case let .belowMinimum(min) = refusal else { return nil }
            return cash.decimal(min)
        }
        if let lowest = minimums.min() { return .belowMinimum(lowest) }

        let maximums = refusals.compactMap { refusal -> Decimal? in
            guard case let .aboveMaximum(max) = refusal else { return nil }
            return cash.decimal(max)
        }
        return maximums.max().map { .aboveMaximum($0) }
    }

    /// The fiat asset a card or bank ask names: the payment country's currency
    /// when a provider takes it, otherwise the first a provider lists.
    var fiatAsset: String {
        let listed = routedCandidates
            .flatMap { $0.routes(rail: rail, direction: direction) }
            .flatMap(\.assets)
        if let currency = country?.currencyCode,
           listed.isEmpty || listed.contains(where: { $0.caseInsensitiveCompare(currency) == .orderedSame }) {
            return currency
        }
        return listed.first ?? country?.currencyCode ?? "USD"
    }

    /// The asset `quote`'s provider-side figures are counted in, with the
    /// decimals the quote carries.
    func quoteUnit(_ quote: FundingQuote) -> FundingAssetUnit {
        FundingAssetUnit(code: ask?.asset ?? session?.choice?.asset ?? fiatAsset, decimals: quote.assetDecimals)
    }
}

// MARK: - Quotes

extension FundingFlowModel {
    var quotedProviders: [(providerId: String, quote: FundingQuote)] {
        rows.compactMap { providerId, state in state.quote.map { (providerId, $0) } }
    }

    var selectedProviderId: String? {
        if let chosenProviderId, rows[chosenProviderId]?.quote != nil { return chosenProviderId }
        return bestProviderId
    }

    var selectedQuote: FundingQuote? {
        selectedProviderId.flatMap { rows[$0]?.quote }
    }

    var isQuoting: Bool {
        rows.values.contains(where: \.isPending) && selectedQuote == nil
    }

    /// What every provider said when none would quote, for the summary screen.
    var unavailableIssue: FundingAmountIssue? {
        guard !rows.isEmpty, quotedProviders.isEmpty, !rows.values.contains(where: \.isPending) else { return nil }
        return refusalForCurrentAmount
    }

    var noProviderQuoted: Bool {
        !rows.isEmpty && quotedProviders.isEmpty && !rows.values.contains(where: \.isPending)
    }

    /// Why no provider quoted, when they all said the same thing: no answer
    /// in time, the payment country, or a reason of their own.
    var quoteFailureText: String? {
        let reasons = rows.values.map { state -> String? in
            guard case let .unavailable(reason) = state else { return nil }
            switch reason {
            case .timeout: return String(localized: .Funding.errorQuoteTimeout)
            case .refused(.countryUnsupported): return String(localized: .Funding.providersCountryUnsupported)
            case let .refused(.other(message)) where !message.isEmpty: return message
            case .refused: return nil
            }
        }
        guard let first = reasons.first, reasons.allSatisfy({ $0 == first }) else { return nil }
        return first
    }
}

// MARK: - Crypto and countries

extension FundingFlowModel {
    var cryptoRoutes: [FundingRoute] {
        candidates.flatMap { $0.routes(rail: .crypto, direction: direction) }
    }

    var networks: [FundingNetwork] {
        var seen: [String] = []
        for id in cryptoRoutes.flatMap({ $0.networks ?? [] }) where !seen.contains(id) {
            seen.append(id)
        }
        return seen.map(FundingNetwork.init(id:))
    }

    /// The lowest minimum a provider has refused with on this network, in
    /// the payment country.
    func minimum(network id: String) -> Decimal? {
        candidates
            .flatMap(\.limits)
            .filter { $0.rail == .crypto && $0.network == id && ($0.country == nil || $0.country == country?.code) }
            .compactMap { cash.decimal($0.min) }
            .min()
    }

    var tokens: [FundingToken] {
        var seen: [String] = []
        let routes = cryptoRoutes.filter { route in
            guard let network else { return true }
            return route.networks.map { $0.contains(network) } ?? true
        }
        for symbol in routes.flatMap(\.assets) where !seen.contains(symbol) {
            seen.append(symbol)
        }
        return seen.map(FundingToken.init(symbol:))
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
