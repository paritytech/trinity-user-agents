import Foundation
import Observation
import TrUAPIHost

/// What the funding overlay's screens and coordinator read from and call on
/// one session's model.
@MainActor
protocol FundingFlowModelProtocol: AnyObject, Observation.Observable {
    // MARK: Session

    var intent: String { get }
    var direction: FundingDirection { get }
    var cash: FundingCash { get }
    var branding: FundingProviderBranding { get }
    var session: FundingSession? { get }
    var progress: FundingProgress? { get }
    var failure: FundingFailure? { get }
    var ended: FundingEndedSession? { get }
    var isStarting: Bool { get }
    var hasStarted: Bool { get }
    var startError: String? { get }
    func hasReached(_ step: FundingStep) -> Bool

    // MARK: Set from outside

    var amountText: String { get set }
    var path: [FundingFlowModel.Screen] { get set }
    var toast: String? { get set }
    var spendable: Decimal? { get set }
    var balanceTask: Task<Void, Never>? { get set }
    var onOutcome: ((FundingPresentOutcome) -> Void)? { get set }
    var onClose: (() -> Void)? { get set }
    var onStartOver: (() -> Void)? { get set }
    var onHistory: (() -> Void)? { get set }

    // MARK: Selection

    var rail: FundingRail { get }
    var country: PaymentCountry? { get }
    var detectedCountry: PaymentCountry? { get }
    var network: String? { get }
    var asset: String? { get }
    var amount: Decimal { get }
    var availableRails: Set<FundingRail> { get }
    var canContinue: Bool { get }
    var amountLimits: (min: Decimal, max: Decimal?) { get }
    var amountIssue: FundingAmountIssue? { get }
    var networks: [FundingNetwork] { get }
    var tokens: [FundingToken] { get }
    var unsupportedCountries: Set<String> { get }
    func minimum(network id: String) -> Decimal?
    func minimum(country code: String) -> Decimal?

    // MARK: Quotes

    var rows: [ProductId: FundingQuoteState] { get }
    var bestProviderId: ProductId? { get }
    var quotesRefreshAt: Date? { get }
    var listedProviderIds: [ProductId] { get }
    var quotedProviders: [(providerId: ProductId, quote: FundingQuote)] { get }
    var selectedProviderId: ProductId? { get }
    var selectedQuote: FundingQuote? { get }
    var chosenQuote: FundingQuote? { get }
    var isQuoting: Bool { get }
    var unavailableIssue: FundingAmountIssue? { get }
    var noProviderQuoted: Bool { get }
    var quoteFailureText: String? { get }
    func brand(for providerId: ProductId) -> FundingProviderBrand
    func quoteUnit(_ quote: FundingQuote) -> FundingAssetUnit

    // MARK: Cancelling

    var isUnpaidTopUp: Bool { get }
    var cancelOffer: (fee: U128, refund: U128)? { get }
    var canRequestCancel: Bool { get }
    var cancelTitle: String { get }
    var isCancelling: Bool { get }
    var isAnsweringOffer: Bool { get }

    // MARK: User actions

    func amountChanged()
    func selectRail(_ newRail: FundingRail)
    func continueFromAmount()
    func chooseNetwork(_ id: String)
    func chooseToken(_ symbol: String)
    func chooseCountry(_ newCountry: PaymentCountry)
    func chooseProvider(_ providerId: ProductId)
    func start()
    func back()
    func close()
    func showHistory()
    func startOver()
    func showProgress()
    func showDeposit()
    func showFees()
    func confirmCancel()
    func requestCancel()
    func answerCancelOffer(accepting terms: CancelTerms?)

    // MARK: Core updates

    /// Shows a session the core holds on its progress screen.
    func resume(_ session: FundingSession)
    func refreshSession()
    func refreshOptions()
    func receive(quotes: FundingQuoteList)
}
