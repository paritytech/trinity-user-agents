import Foundation
import FoundationExt
import StructuredConcurrencyTestSupport
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

/// Sessions as the core hands them to the funding screens.
enum FundingFixtures {
    static let entryId = "intent"
    /// The wall clock the screens read, in the core's milliseconds.
    static let nowMs: UInt64 = 1_700_000_000_000

    static func provider(_ name: String) -> ProductId {
        try! ProductId("\(name).dot")
    }

    static func quote(_ id: String = "quote", send: U128 = "5200") -> FundingQuote {
        FundingQuote(
            quoteId: id,
            sendAmount: send,
            receiveAmount: "50000000",
            providerFee: "0",
            networkFee: "0",
            etaSecs: nil,
            expiresAt: nil,
            assetDecimals: 2
        )
    }

    /// The core's list for the latest ask: each provider's answer, by name.
    static func quotes(_ rows: [(provider: String, state: FundingQuoteState)], best: String?) -> FundingQuoteList {
        FundingQuoteList(
            requestId: 1,
            rows: rows.map { FundingQuoteRow(providerId: provider($0.provider), state: $0.state) },
            best: best.map(provider),
            refreshAtMs: nowMs + 60_000,
            amount: "50000000",
            refusal: nil,
            issue: nil,
            complete: true
        )
    }

    static func offering(
        rails: [FundingRail] = [.card, .bank, .crypto],
        detectedCountry: String = "DE"
    ) -> FundingOptions {
        FundingOptions(
            candidates: [],
            countries: [],
            limits: FundingAmountLimits(min: "1000000", max: nil),
            networks: [FundingNetworkLimits(network: "polkadot", limits: FundingAmountLimits(min: nil, max: nil))],
            rails: rails,
            asset: "EUR",
            tokens: ["USDC"],
            detectedCountry: detectedCountry
        )
    }

    /// A session being set up: what the core offers, and the quotes for the
    /// latest ask.
    static func selecting(
        _ direction: FundingDirection = .in,
        options: FundingOptions = offering(),
        quotes: FundingQuoteList? = nil
    ) -> FundingEntry {
        FundingEntry(
            id: entryId,
            direction: direction,
            amount: nil,
            session: nil,
            progress: nil,
            summary: nil,
            options: options,
            quotes: quotes,
            isQuoting: false
        )
    }

    /// A session handed to a provider, in flight or ended by `stage`.
    static func started(
        rail: FundingRail = .card,
        stage: FundingStage = .open,
        progress: FundingProgress? = progressState()
    ) -> FundingEntry {
        let session = FundingSession(
            intent: entryId,
            ownerProductId: nil,
            direction: .in,
            amount: "50000000",
            stage: stage,
            openedAtMs: nowMs,
            deadlineMs: nowMs + 86_400_000,
            acknowledged: false,
            providerId: provider("ramp"),
            assignedAtMs: nowMs,
            cancel: nil,
            updates: [],
            started: [],
            choice: FundingChoice(quote: quote(), rail: rail, asset: rail == .crypto ? "USDC" : "EUR", network: nil),
            refund: nil,
            detail: nil
        )
        return FundingEntry(
            id: entryId,
            direction: .in,
            amount: nil,
            session: session,
            progress: progress,
            summary: nil,
            options: nil,
            quotes: nil,
            isQuoting: false
        )
    }

    static func progressState(
        reached: [FundingStep] = [.started],
        deposit: FundingDeposit? = nil,
        status: FundingStatus = .waitingForPayment,
        longerAtMs: UInt64? = nil,
        cancel: FundingCancelAvailability = .available
    ) -> FundingProgress {
        FundingProgress(
            steps: [FundingStep.started, .received, .added].map { step in
                FundingProgressStep(step: step, reachedAtMs: reached.contains(step) ? nowMs : nil)
            },
            band: .awaitingPayment,
            lastHeardAtMs: nil,
            detail: nil,
            failedAtMs: nil,
            transactionId: nil,
            reference: nil,
            deposit: deposit,
            mismatch: nil,
            retrying: false,
            payout: nil,
            refunded: nil,
            status: status,
            awaitsUserTransfer: false,
            longerAtMs: longerAtMs,
            moneyMoved: false,
            cancel: cancel
        )
    }

    static let cryptoDeposit = FundingDeposit.crypto(
        address: "0xaddress",
        network: "polkadot",
        asset: "USDC",
        amount: "5200",
        decimals: 2,
        exact: true,
        uri: nil,
        expiresAt: nil
    )
}

/// A wall clock that reads `FundingFixtures.nowMs`.
struct FixedFundingDate: CurrentDateProviding {
    let now = Date(milliseconds: FundingFixtures.nowMs)
}

/// One funding flow's screens over a mocked core, wireframe and view: each
/// presenter is assembled as its view factory does, on the real flow state
/// and view model factory.
@MainActor
final class FundingScreenHarness {
    let service = MockFundingService()
    let wireframe = MockFundingFlowWireframe()
    let view = MockFundingScreenView()
    let environment = MockFundingFlowEnvironment()
    let clock = ManualClock()
    let flowState: FundingFlowState
    let viewModelFactory: FundingViewModelFactory

    init(entry: FundingEntry) {
        flowState = FundingFlowState(
            entry: entry,
            cashInfo: environment.cashInfo,
            branding: environment.branding,
            iconLoader: nil
        )
        viewModelFactory = FundingViewModelFactory(cashInfo: environment.cashInfo, cashSymbol: environment.cashSymbol)
    }

    func amount() -> FundingAmountPresenter {
        let interactor = FundingAmountInteractor(service: service, entryId: entryId, environment: environment)
        let presenter = FundingAmountPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory,
            clock: clock
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func summary() -> FundingSummaryPresenter {
        let interactor = FundingSummaryInteractor(service: service, entryId: entryId)
        let presenter = FundingSummaryPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory,
            logger: Logger.shared
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func providers() -> FundingProvidersPresenter {
        let interactor = FundingProvidersInteractor(service: service, entryId: entryId)
        let presenter = FundingProvidersPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func country() -> FundingCountryPresenter {
        let interactor = FundingCountryInteractor(service: service, entryId: entryId)
        let presenter = FundingCountryPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func network() -> FundingNetworkPresenter {
        let interactor = FundingNetworkInteractor(service: service, entryId: entryId)
        let presenter = FundingNetworkPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func token() -> FundingTokenPresenter {
        let interactor = FundingTokenInteractor(service: service, entryId: entryId)
        let presenter = FundingTokenPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func deposit() -> FundingDepositPresenter {
        let interactor = FundingDepositInteractor(service: service, entryId: entryId)
        let presenter = FundingDepositPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    func progress() -> FundingProgressPresenter {
        let interactor = FundingProgressInteractor(
            service: service,
            entryId: entryId,
            clock: clock,
            dateProvider: FixedFundingDate()
        )
        let presenter = FundingProgressPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        interactor.presenter = presenter
        presenter.view = view
        return presenter
    }

    func cancelConfirm() -> FundingCancelConfirmPresenter {
        let interactor = FundingCancelConfirmInteractor(service: service, entryId: entryId)
        let presenter = FundingCancelConfirmPresenter(
            flowState: flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory
        )
        interactor.entryOutput = presenter
        presenter.view = view
        return presenter
    }

    private var entryId: String {
        flowState.entryId
    }
}
