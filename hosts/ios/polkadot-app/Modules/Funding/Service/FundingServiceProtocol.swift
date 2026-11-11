import AsyncExtensions
import Foundation
import TrUAPIHost

/// Everything the funding screens ask of the core, in one place. The core
/// owns every session: the screens follow a session's state, report what the
/// user picked, and leave every decision to the core.
///
/// Every screen's interactor uses the same instance.
protocol FundingServiceProtocol: AnyObject, Sendable {
    /// Every session the core holds, in flight and ended, newest first. Yields
    /// the current list first, then a new one on every change.
    func subscribeEntries() -> AnyAsyncSequence<[FundingEntry]>

    /// Session `entryId` as it changes, current state first. Ends once the
    /// session ended, or once the core dropped a session that never started.
    func subscribeEntryState(_ entryId: String) -> AnyAsyncSequence<FundingEntry>

    /// Opens a session from the host's side. Its screens are asked for
    /// through ``events`` as `presentationRequested`; returns once the core
    /// answered whether the session started.
    func open(_ direction: FundingDirection, amount: U128?) async throws

    /// Tells the core what the user picked. What it offers for it comes back
    /// on the session's state, as `.selecting(options)`.
    func select(_ selection: FundingSelection, for entryId: String) async

    /// Asks every provider routing the reported selection to quote `amount`,
    /// or drops the quotes on screen when `amount` is nil. The quotes come
    /// back on the session's state, only for the latest ask.
    func quote(amount: U128?, for entryId: String)

    /// Why `amount` cannot go ahead for the reported selection, given what a
    /// withdrawal can spend.
    func amountIssue(_ amount: U128, spendable: U128?, for entryId: String) -> FundingAmountIssue?

    /// Hands the session to `provider` on the quote on screen. With no
    /// provider, the core hands it to the latest ask's best quote once every
    /// provider answered, and the session's state says when it did.
    func start(_ entryId: String, provider: ProductId?) async throws

    /// The user left the screen that started the session on its best quote:
    /// the core no longer starts it.
    func stopStarting(_ entryId: String)

    /// Cancels a session. One that never started is dropped at once, as when
    /// the user leaves its screens; one in flight goes to the core, which ends
    /// it or asks the provider.
    func cancel(_ entryId: String) async throws -> CancelOutcome

    /// Answers the provider's cancel offer: `terms` are the ones the user
    /// accepted, as shown, or nil to keep going.
    func answerCancelOffer(_ entryId: String, accepting terms: CancelTerms?) async throws -> CancelConfirmation

    /// The user was shown how session `entryId` ended.
    func acknowledge(_ entryId: String) async

    /// What the core asks of the app's screens. One consumer, the main tab
    /// bar, reads it; events wait until it does.
    var events: AnyAsyncSequence<FundingEvent> { get }

    /// Lets the core present the screen `request` asked for, once the user
    /// is looking at its session or tapped the notice about it.
    func openProviderFrame(_ request: FundingFrameRequest)

    /// Answers the provider's screen `frame`, once it is gone.
    func answer(frame: FundingFrame.ID, with outcome: FundingFrameOutcome)
}

/// What the core asks of the app's screens.
enum FundingEvent: Sendable {
    /// Show the funding flow for this new session.
    case presentationRequested(FundingEntry)
    /// A provider wants to show one of its screens for a session. The app
    /// answers with ``FundingServiceProtocol/openProviderFrame(_:)`` at once
    /// when that session is on screen, or after the user tapped a notice.
    case frameRequested(FundingFrameRequest)
    /// The core presents a provider's screen now; answered with
    /// ``FundingServiceProtocol/answer(frame:with:)``.
    case frameOpened(FundingFrame)
    /// The provider's screen for a session is done: take it down and show the
    /// session in its place.
    case frameClosed(entryId: String)
    /// A provider's worker keeps crashing while the core restarts it.
    case providerNotWorking(ProductId)
}

/// A provider's ask to show one of its screens for session `entryId`.
struct FundingFrameRequest: Equatable, Sendable {
    let entryId: String
    let providerId: ProductId
}

/// A provider's screen the core is presenting.
struct FundingFrame: Equatable, Sendable, Identifiable {
    let id: UUID
    let entryId: String
    let providerId: ProductId
    let target: FundingFrameTarget
    /// The session waits on the user's own transfer, so the screen offers to
    /// say the funds were sent.
    let awaitsUserTransfer: Bool
}

/// Why the session could not be handed to the chosen provider.
enum FundingStartError: Error, Equatable {
    /// No quote from that provider is on screen.
    case noQuote
    /// The user left before the provider took the session.
    case left
    /// The provider would not take it; the session's failure says why when
    /// the core recorded one.
    case notSelected(FundingFailure?)
}
