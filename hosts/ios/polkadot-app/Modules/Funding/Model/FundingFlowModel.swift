import Foundation
import Observation
import TrUAPIHost

/// One funding overlay: the session the core opened, what the user has chosen
/// so far, and the quotes the providers sent back.
///
/// The core keeps the session live while the overlay is up, so the provider is
/// chosen and selected here before the overlay answers `Started`. From there
/// the overlay shows the session's progress until the user leaves; crypto
/// value in shows its deposit screen first, over the progress screen.
///
/// Only the state the screens draw lives on the main actor: every call to
/// the core and the providers goes through ``FundingFlowServiceProtocol``.
@MainActor
@Observable
final class FundingFlowModel: FundingFlowModelProtocol {
    let intent: String
    let direction: FundingDirection
    let cash: FundingCash
    /// What a payment can spend now, once the app has read it.
    var spendable: Decimal? {
        didSet { refreshAmountIssue() }
    }

    /// Keeps ``spendable`` current while the overlay is up.
    var balanceTask: Task<Void, Never>?

    private(set) var rail: FundingRail
    var amountText: String {
        didSet { refreshAmountIssue() }
    }

    /// Why the typed amount cannot go ahead, as the core decides it from the
    /// reported selection's limits, its latest quote list and what a
    /// withdrawal can spend.
    private(set) var amountIssue: FundingAmountIssue?
    var path: [Screen] = []
    /// Where the user pays from: the device's region until the user picks one.
    private(set) var country: PaymentCountry?
    private(set) var network: String?
    private(set) var asset: String?
    private(set) var chosenProviderId: ProductId?
    var toast: String?

    private(set) var candidates: [FundingCandidate]
    /// The rails the core says a provider routes, in its order.
    private(set) var rails: [FundingRail]
    /// The asset the core says an ask for the reported selection names.
    private(set) var offeredAsset: String?
    /// The tokens the core says a crypto session can use on the picked network.
    private(set) var tokenSymbols: [String] = []
    /// The amounts the core says the reported selection takes across every provider.
    private(set) var limits = FundingAmountLimits(min: nil, max: nil)
    /// The same for each network a crypto route declares.
    private(set) var networkLimits: [FundingNetworkLimits] = []
    /// What the core knows about each country on the reported rail and asset, by code.
    private(set) var countrySupport: [String: FundingCountrySupport] = [:]
    /// The country the device is set to, which the country list leads with.
    private(set) var detectedCountry: PaymentCountry?
    private(set) var isStarting = false
    private(set) var hasStarted = false
    /// Why the chosen provider could not be handed the session.
    private(set) var startError: String?
    private(set) var isCancelling = false
    private(set) var isAnsweringOffer = false
    private(set) var progress: FundingProgress?
    private(set) var session: FundingSession?

    /// Answers the core's `present_funding`; called once.
    var onOutcome: ((FundingPresentOutcome) -> Void)?
    /// Takes the overlay down.
    var onClose: (() -> Void)?
    /// Takes the overlay down and opens a new session the same way.
    var onStartOver: (() -> Void)?
    /// Leaves the overlay for the history of every session.
    var onHistory: (() -> Void)?

    /// The ask on screen and the core's quotes for it.
    let quotes: FundingQuotes
    /// The providers' brands, read once each.
    let brands: FundingBrands

    let service: FundingFlowServiceProtocol
    /// Makes the overlay's core calls in the order the user took its steps.
    let coreCalls: FundingCoreCallQueue
    private let logger: LoggerProtocol
    /// The deposit screen asked the core to start the session on the best
    /// quote once every provider answered.
    @ObservationIgnored private var startsWhenQuoted = false
    /// Set when the user left before `Started`, which the core discards the
    /// session on.
    @ObservationIgnored private var hasLeft = false
    /// The selection the core was last told of.
    @ObservationIgnored private var reportedSelection: FundingSelection?
    /// Set once the user picked a country, which detection then leaves alone.
    @ObservationIgnored private var hasPickedCountry = false

    var branding: FundingProviderBranding {
        service.branding
    }

    init(
        context: Context,
        service: FundingFlowServiceProtocol,
        logger: LoggerProtocol,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        intent = context.intent
        direction = context.direction
        cash = context.cash
        self.service = service
        let coreCalls = FundingCoreCallQueue(service: service)
        self.coreCalls = coreCalls
        self.logger = logger
        quotes = FundingQuotes(intent: context.intent, coreCalls: coreCalls, clock: clock)
        brands = FundingBrands(service: service)
        amountText = context.amount.flatMap { context.cash.decimal($0) }.map(Self.text(for:)) ?? ""

        let options = service.options(intent: context.intent)
        candidates = options?.candidates ?? []
        rails = options?.rails ?? []
        rail = options?.rails.first ?? .card
        detectedCountry = (options?.detectedCountry ?? FundingCountries.localeRegion())
            .flatMap { FundingCountries.country(code: $0) }
        country = detectedCountry
        quotes.onShown = { [weak self] in self?.refreshAmountIssue() }
        if let options {
            apply(options)
        } else {
            reportSelection()
        }
        refreshAmountIssue()
    }
}

// MARK: - Navigation

extension FundingFlowModel {
    func selectRail(_ newRail: FundingRail) {
        guard newRail != rail else { return }

        rail = newRail
        network = nil
        asset = nil
        offeredAsset = nil
        quotes.drop()
        reportSelection()
        amountChanged()
    }

    func continueFromAmount() {
        guard canContinue else { return }

        quotes.cancelPause()
        switch rail {
        case .crypto:
            path.append(.network)
        case .card,
             .bank:
            requestQuote()
            path.append(.summary)
        }
    }

    func chooseNetwork(_ id: String) {
        network = id
        asset = nil
        reportSelection()
        path.append(.token)
    }

    func chooseToken(_ symbol: String) {
        asset = symbol
        if direction == .in {
            startsWhenQuoted = true
            requestQuote()
            path.append(.deposit)
        } else {
            requestQuote()
            path.append(.summary)
        }
    }

    func chooseCountry(_ newCountry: PaymentCountry) {
        hasPickedCountry = true
        country = newCountry
        path.removeLast()
        reportSelection()
        requestQuote()
    }

    func chooseProvider(_ providerId: ProductId) {
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
        stopStartingWhenQuoted()
    }

    /// The user left the overlay. Before `Started` that is the answer the core
    /// discards the session on; after it the session runs on without the
    /// overlay, and only ``requestCancel()`` ends it.
    func close() {
        leave()
        onClose?()
    }

    /// Leaves the overlay, as ``close()`` does, for the history.
    func showHistory() {
        leave()
        onHistory?()
    }

    /// An ended session leaves for a new one in the same direction, from the
    /// start.
    func startOver() {
        quotes.cancelPause()
        balanceTask?.cancel()
        onStartOver?()
    }

    func confirmCancel() {
        path.append(.cancelConfirm)
    }

    /// Asks the core to cancel. It ends a session no money moved on, and
    /// otherwise asks the provider, who answers with an offer or a refusal.
    func requestCancel() {
        guard !isCancelling else { return }

        isCancelling = true
        Task { [service, intent] in
            defer { isCancelling = false }
            do {
                var outcome = try await service.cancel(intent: intent)
                if outcome == .requested, isUnpaidTopUp {
                    // Nothing was paid, so the second cancel ends the session.
                    outcome = try await service.cancel(intent: intent)
                }
                switch outcome.followUp {
                case .close:
                    close()
                case let .toast(message):
                    leaveCancelConfirm(toast: message)
                    refreshSession()
                }
            } catch {
                logger.error("[funding] \(intent): cancel failed: \(error)")
                leaveCancelConfirm(toast: String(localized: .Funding.errorCancelFailed))
            }
        }
    }

    /// Answers the provider's cancel offer: `terms` are the ones the user
    /// accepted, as shown, or `nil` to keep going. The core accepts only terms
    /// that still match the offer.
    func answerCancelOffer(accepting terms: CancelTerms?) {
        guard !isAnsweringOffer else { return }

        isAnsweringOffer = true
        Task { [service, intent] in
            defer { isAnsweringOffer = false }
            do {
                let answer = try await service.confirmCancel(intent: intent, terms: terms)
                toast = answer.toast
            } catch {
                logger.error("[funding] \(intent): answering the cancel offer failed: \(error)")
                toast = String(localized: .Funding.errorCancelFailed)
            }
            refreshSession()
        }
    }

    /// Shows a session that was left running, or has ended, on its progress
    /// screen, with the crypto deposit over it while the funds have not been
    /// seen.
    func resume(_ session: FundingSession) {
        rail = session.choice?.rail ?? rail
        asset = session.choice?.asset
        chosenProviderId = session.providerId
        brands.load([session.providerId].compactMap(\.self))
        hasStarted = true
        refreshSession()
        path = [.progress]
        if showsDepositFirst, session.stage.isOpen, !hasReached(.received) { path.append(.deposit) }
    }

    /// Back to the progress screen, from the deposit screen over it or from
    /// the screen the session was started on.
    func showProgress() {
        if let index = path.firstIndex(of: .progress) {
            path.removeSubrange(path.index(after: index)...)
        } else {
            path = [.progress]
        }
    }

    func showDeposit() {
        path.append(.deposit)
    }

    func showFees() {
        path.append(.fees)
    }

    /// Called on every keypad change: quotes the new amount once the user
    /// pauses, so the screen can show a provider's limit before Continue.
    func amountChanged() {
        quotes.cancelPause()
        guard rail != .crypto, amount > 0 else { return }

        quotes.askAfterPause { [weak self] in self?.requestQuote() }
    }

    /// Reads the core's issue with the typed amount again.
    func refreshAmountIssue() {
        amountIssue = amountUnits.flatMap {
            service.amountIssue(intent: intent, amount: $0, spendable: spendable.flatMap { cash.units($0) })
        }
    }
}

// MARK: - Start

extension FundingFlowModel {
    /// Hands the session to the provider whose quote the user is looking at,
    /// then answers `Started`. A provider is only chosen on its quote.
    ///
    /// The user can leave while the provider is being selected. The overlay
    /// has answered `.dismissed` by then, which the core discards the session
    /// on, so the selection is neither reported started nor shown.
    func start() {
        guard !isStarting, !hasStarted, let providerId = selectedProviderId,
              let quoteId = rows[providerId]?.quote?.quoteId
        else { return }

        isStarting = true
        startError = nil

        Task { [service, intent] in
            defer { isStarting = false }
            do {
                let selected = try await service.selectProvider(providerId, quoteId: quoteId, intent: intent)
                guard !hasLeft else { return }
                guard selected else {
                    refreshSession()
                    startError = failure?.failedText ?? String(localized: .Funding.errorStartFailed)
                    return
                }
            } catch {
                logger.error("[funding] \(intent): selecting \(providerId) failed: \(error)")
                if !hasLeft { startError = String(localized: .Funding.errorStartFailed) }
                return
            }

            didStart()
        }
    }

    /// The core pushed the session again: draw what it holds now. A session
    /// the core started on its best quote is reported started.
    func refreshSession() {
        session = service.session(intent: intent)
        progress = service.progress(intent: intent)
        if startsWhenQuoted, !hasStarted, !hasLeft, session?.providerId != nil {
            startsWhenQuoted = false
            didStart()
        }
    }

    /// The core pushed what the overlay offers again: its providers, and the
    /// countries and amounts for the reported selection.
    func refreshOptions() {
        apply(service.options(intent: intent))
    }
}

// MARK: - Private

private extension FundingFlowModel {
    static func text(for value: Decimal) -> String {
        NSDecimalNumber(decimal: value.fundingRounded(scale: 2, mode: .down)).stringValue
    }

    /// Crypto value in is paid from its deposit screen, which the session
    /// starts on.
    var showsDepositFirst: Bool {
        direction == .in && rail == .crypto
    }

    func leave() {
        quotes.cancelPause()
        balanceTask?.cancel()
        stopStartingWhenQuoted()
        guard !hasStarted, !hasLeft else { return }

        hasLeft = true
        onOutcome?(.dismissed)
    }

    func leaveCancelConfirm(toast message: String) {
        if path.last == .cancelConfirm { path.removeLast() }
        toast = message
    }

    func didStart() {
        guard !hasStarted else { return }

        hasStarted = true
        onOutcome?(.started)
        refreshSession()

        if showsDepositFirst {
            if path.last != .deposit { path.append(.deposit) }
        } else {
            path = [.progress]
        }
    }

    /// The user left the deposit screen before the session started: the
    /// core no longer starts it on its best quote.
    func stopStartingWhenQuoted() {
        guard startsWhenQuoted else { return }

        startsWhenQuoted = false
        coreCalls.run { [intent] service in await service.cancelAutoStart(intent: intent) }
    }

    /// Takes what the core offers for the reported selection, and reports
    /// the selection again when that changes it.
    func apply(_ options: FundingOptions?) {
        guard let options else { return }

        candidates = options.candidates
        rails = options.rails
        offeredAsset = options.asset
        tokenSymbols = options.tokens
        countrySupport = Dictionary(
            options.countries.map { ($0.country.uppercased(), $0.support) }
        ) { _, latest in latest }
        limits = options.limits
        networkLimits = options.networks
        brands.load(candidates.map(\.providerId))
        takeDetectedCountry(options.detectedCountry)
        refreshAmountIssue()
        reportSelection()
    }

    /// The core's detection of the user's country leads the country list,
    /// and is the one paid from until the user picks one.
    func takeDetectedCountry(_ code: String?) {
        guard let code, code != detectedCountry?.code,
              let detected = FundingCountries.country(code: code)
        else { return }

        detectedCountry = detected
        if !hasPickedCountry { country = detected }
    }

    /// Tells the core the rail, asset, network and country on screen when
    /// they changed, and takes what it offers for them.
    func reportSelection() {
        let selection = FundingSelection(
            rail: rail,
            asset: rail == .crypto ? asset : nil,
            network: rail == .crypto ? network : nil,
            country: country?.code,
            currency: country?.currencyCode
        )
        guard !hasStarted, selection != reportedSelection else { return }

        reportedSelection = selection
        coreCalls.run { [intent] service in
            await service.select(selection, intent: intent)
        } then: { [weak self] options in
            self?.apply(options)
        }
    }

    /// Asks the core to price the typed amount on the selection on screen,
    /// which it is told of first. The core keeps the running request when it
    /// already prices that.
    func requestQuote() {
        reportSelection()
        quotes.request(amount: amountUnits, startWhenQuoted: startsWhenQuoted)
    }
}
