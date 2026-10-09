import Foundation
import TrUAPIHost

/// Why the typed amount cannot go ahead.
enum FundingAmountIssue: Equatable {
    case belowMinimum(Decimal)
    case aboveMaximum(Decimal)
    case notEnoughBalance
}

extension FundingFlowModel {
    var amount: Decimal {
        Decimal(string: amountText, locale: Locale(identifier: "en_US")) ?? 0
    }

    var availableRails: Set<FundingRail> {
        Set(FundingRail.displayOrder.filter { rail in
            candidates.contains { $0.serves(rail: rail, direction: direction) }
        })
    }

    var servingCandidates: [FundingCandidate] {
        candidates.filter { $0.serves(rail: rail, direction: direction) }
    }

    var canContinue: Bool {
        amount > 0 && amountIssue == nil && availableRails.contains(rail)
    }

    /// The lowest minimum and highest maximum any provider has refused with
    /// for this rail, which the core learns and keeps on the candidates. No
    /// provider refusing yet means no limit to show.
    var learnedLimits: (min: Decimal?, max: Decimal?) {
        let limits = servingCandidates
            .flatMap(\.limits)
            .filter { limit in
                limit.rail == rail
                    && (asset == nil || limit.asset.caseInsensitiveCompare(asset ?? "") == .orderedSame)
                    && (network == nil || limit.network == nil || limit.network == network)
            }

        let minimum = limits.compactMap { cash.decimal($0.min) }.min()
        let maximum = limits.compactMap { cash.decimal($0.max) }.max()
        return (minimum, maximum)
    }

    var amountIssue: FundingAmountIssue? {
        if direction == .out, let spendable, amount > spendable {
            return .notEnoughBalance
        }

        if let refused = refusalForCurrentAmount {
            return refused
        }

        let limits = learnedLimits
        if amount > 0, let minimum = limits.min, amount < minimum {
            return .belowMinimum(minimum)
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
        let listed = servingCandidates
            .flatMap { $0.routes(rail: rail, direction: direction) }
            .flatMap(\.assets)
        if let currency = country?.currencyCode,
           listed.isEmpty || listed.contains(where: { $0.caseInsensitiveCompare(currency) == .orderedSame }) {
            return currency
        }
        return listed.first ?? country?.currencyCode ?? "USD"
    }

    /// The asset the quote's provider-side figures are counted in.
    var quoteUnit: FundingAssetUnit {
        FundingAssetUnit(code: ask?.asset ?? fiatAsset)
    }
}

// MARK: - Quotes

extension FundingFlowModel {
    var quotedProviders: [(providerId: String, quote: FundingQuote)] {
        rows.compactMap { providerId, state in state.quote.map { (providerId, $0) } }
    }

    /// The cheapest quote: least to pay for value in, most received for value
    /// out.
    var bestProviderId: String? {
        let quoted = quotedProviders
        switch direction {
        case .in:
            return quoted.min { quoteUnit.decimal($0.quote.sendAmount) < quoteUnit.decimal($1.quote.sendAmount) }?
                .providerId
        case .out:
            return quoted.max { quoteUnit.decimal($0.quote.receiveAmount) < quoteUnit.decimal($1.quote.receiveAmount) }?
                .providerId
        }
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

    var quoteExpiry: Date? {
        quotedProviders
            .compactMap(\.quote.expiresAt)
            .min()
            .map { Date(timeIntervalSince1970: TimeInterval($0) / 1_000) }
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

    /// The lowest minimum a provider has refused with on this network.
    func minimum(network id: String) -> Decimal? {
        candidates
            .flatMap(\.limits)
            .filter { $0.rail == .crypto && $0.network == id }
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

    /// Countries no provider serving this rail takes payment from.
    var unsupportedCountries: Set<String> {
        let serving = servingCandidates
        guard !serving.isEmpty else { return [] }

        let perProvider = serving.map { candidate in
            Set(candidate.unsupported.filter { $0.rail == rail }.compactMap { $0.country?.uppercased() })
        }
        var refused = perProvider.dropFirst().reduce(perProvider[0]) { $0.intersection($1) }

        let everyRowRefusedCountry = !rows.isEmpty && rows.values.allSatisfy { $0.refusal == .countryUnsupported }
        if everyRowRefusedCountry, let code = ask?.country {
            refused.insert(code.uppercased())
        }
        return refused
    }
}
