import Foundation
import Observation
import TrUAPIHost

/// One funding overlay: the session the core opened, what the user has chosen
/// so far, and the quotes the providers sent back.
///
/// The core keeps the session live while the overlay is up, so the provider is
/// chosen and selected here before the overlay answers `Started`. Card ends
/// the overlay there; bank and crypto keep it on the deposit screen, which
/// follows the session until the funds arrive or the user leaves.
@MainActor
@Observable
final class FundingFlowModel {
    enum Screen: Hashable {
        case summary
        case fees
        case country
        case providers
        case network
        case token
        case deposit
        case cancelConfirm
    }

    struct Context {
        let intent: String
        let direction: FundingDirection
        let amount: U128?
        let cash: FundingCash
        let spendable: Decimal?
    }

    let intent: String
    let direction: FundingDirection
    let cash: FundingCash
    let spendable: Decimal?

    var rail: FundingRail
    var amountText: String
    var path: [Screen] = []
    var country: FundingCountry?
    var network: String?
    var asset: String?
    var chosenProviderId: String?
    var toast: String?

    private(set) var candidates: [FundingCandidate]
    private(set) var rows: [String: FundingQuoteState] = [:]
    private(set) var ask: FundingQuoteAsk?
    private(set) var brands: [String: FundingProviderBrand] = [:]
    private(set) var isStarting = false
    private(set) var hasStarted = false
    private(set) var startFailed = false
    private(set) var progress: FundingProgress?
    private(set) var session: FundingSession?

    /// Answers the core's `present_funding`; called once.
    var onOutcome: ((FundingPresentOutcome) -> Void)?
    /// Takes the overlay down.
    var onClose: (() -> Void)?

    let runtime: FundingRuntime
    private let branding: FundingProviderBranding
    private var startsWhenQuoted = false
    private var quoteTask: Task<Void, Never>?

    init(
        context: Context,
        runtime: FundingRuntime,
        branding: FundingProviderBranding,
        country: FundingCountry? = FundingCountries.detected()
    ) {
        intent = context.intent
        direction = context.direction
        cash = context.cash
        spendable = context.spendable
        self.runtime = runtime
        self.branding = branding
        self.country = country
        amountText = context.amount.map { Self.text(for: context.cash.decimal($0)) } ?? ""

        let candidates = runtime.fundingCandidates(intent: context.intent)
        self.candidates = candidates
        rail = Self.defaultRail(candidates: candidates, direction: context.direction)
        loadBrands()
    }

    var brandsInOrder: [String] {
        candidates.map(\.providerId)
    }

    func brand(for providerId: String) -> FundingProviderBrand {
        brands[providerId] ?? .placeholder(for: providerId)
    }
}

// MARK: - Navigation

extension FundingFlowModel {
    func selectRail(_ newRail: FundingRail) {
        guard newRail != rail else { return }

        rail = newRail
        network = nil
        asset = nil
        rows = [:]
        ask = nil
        amountChanged()
    }

    func continueFromAmount() {
        guard canContinue else { return }

        switch rail {
        case .crypto:
            path.append(.network)
        case .card,
             .bank:
            if ask != currentAsk() { requestQuote() }
            path.append(.summary)
        }
    }

    func chooseNetwork(_ id: String) {
        network = id
        asset = nil
        path.append(.token)
    }

    func chooseToken(_ symbol: String) {
        asset = symbol
        requestQuote()

        if direction == .in {
            startsWhenQuoted = true
            path.append(.deposit)
        } else {
            path.append(.summary)
        }
    }

    func chooseCountry(_ newCountry: FundingCountry) {
        country = newCountry
        path.removeLast()
        requestQuote()
    }

    func chooseProvider(_ providerId: String) {
        chosenProviderId = providerId
        toast = String(localized: .Funding.providerChanged)
        path.removeLast()
    }

    func back() {
        guard !path.isEmpty else {
            close()
            return
        }

        path.removeLast()
    }

    /// The user left the overlay. Before `Started` that is the answer the core
    /// discards the session on; after it the session runs on without the
    /// overlay.
    func close() {
        quoteTask?.cancel()
        if !hasStarted { onOutcome?(.dismissed) }
        onClose?()
    }

    func confirmCancel() {
        path.append(.cancelConfirm)
    }

    func cancelTopUp() {
        Task {
            _ = try? await runtime.cancelFunding(intent: intent)
            close()
        }
    }
}

// MARK: - Start

extension FundingFlowModel {
    /// Hands the session to the provider whose quote the user is looking at,
    /// then answers `Started`.
    func start() {
        guard !isStarting, !hasStarted, let providerId = selectedProviderId else { return }

        isStarting = true
        startFailed = false
        let quoteId = rows[providerId]?.quote?.quoteId

        Task {
            let selected = (try? await runtime.selectFundingProvider(
                intent: intent,
                providerId: providerId,
                quoteId: quoteId
            )) ?? false

            isStarting = false
            guard selected else {
                startFailed = true
                return
            }

            didStart()
        }
    }

    /// The core's view of the session moved on: read it again.
    func refreshSession() {
        session = runtime.fundingSession(intent: intent)
        progress = runtime.fundingProgress(intent: intent)
    }

    func receive(row: FundingQuoteRow) {
        rows[row.providerId] = row.state

        guard startsWhenQuoted, !isStarting, !hasStarted, !rows.values.contains(where: \.isPending) else { return }

        startsWhenQuoted = false
        start()
    }
}

// MARK: - Private

private extension FundingFlowModel {
    static func defaultRail(candidates: [FundingCandidate], direction: FundingDirection) -> FundingRail {
        let served = FundingRail.displayOrder.filter { rail in
            candidates.contains { $0.serves(rail: rail, direction: direction) }
        }
        return served.contains(.card) ? .card : served.first ?? .card
    }

    static func text(for value: Decimal) -> String {
        NSDecimalNumber(decimal: value.fundingRounded(scale: 2, mode: .down)).stringValue
    }

    func didStart() {
        hasStarted = true
        onOutcome?(.started)
        refreshSession()

        switch rail {
        case .card:
            onClose?()
        case .bank,
             .crypto:
            if path.last != .deposit { path.append(.deposit) }
        }
    }

    func loadBrands() {
        for providerId in candidates.map(\.providerId) {
            Task { [branding] in
                let brand = await branding.brand(for: providerId)
                brands[providerId] = brand
            }
        }
    }
}

// MARK: - Quoting

extension FundingFlowModel {
    /// Called on every keypad change: quotes the new amount once the user
    /// pauses, so the screen can show a provider's limit before Continue.
    func amountChanged() {
        quoteTask?.cancel()
        guard rail != .crypto, amount > 0 else { return }

        quoteTask = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }

            requestQuote()
        }
    }

    func requestQuote() {
        guard let newAsk = currentAsk() else { return }

        ask = newAsk
        rows = Dictionary(uniqueKeysWithValues: servingCandidates.map { ($0.providerId, .pending) })
        runtime.getFundingQuote(intent: intent, ask: newAsk)
    }

    /// Asks again once the quote on screen expires.
    func refreshIfExpired(now: Date = Date()) {
        guard let expiry = quoteExpiry, expiry <= now, !rows.values.contains(where: \.isPending) else { return }

        requestQuote()
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
}
