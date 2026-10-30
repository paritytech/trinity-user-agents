import Foundation
import TrUAPIHost

/// What the funding overlay UI implements: the screen a session is started
/// from, the frame a provider's own screens are shown in, and the updates
/// those screens and the in-flight pill draw from.
///
/// The core keeps the session live while `presentFunding` waits, so the UI
/// quotes it and selects a provider through the runtime before it answers
/// `.started`.
protocol FundingOverlayPresenting: AnyObject, Sendable {
    func presentFunding(
        productId: String?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async -> FundingPresentOutcome

    func presentProviderFrame(providerId: String, intent: String, target: FundingFrameTarget) async
        -> FundingFrameOutcome

    /// The provider closed its frame for session `intent`: answer it
    /// `.closed` and show the session sheet in its place.
    func closeProviderFrame(providerId: String, intent: String)

    func fundingSessionChanged(intent: String, status: HostFundingStatusSubscribeItem)

    /// What session `intent`'s overlay offers changed in the session book.
    func fundingOptionsChanged(intent: String)

    func fundingQuotesChanged(intent: String, quotes: FundingQuoteList)

    /// Shows session `intent` again after the user left it running.
    func reopen(intent: String)

    /// Shows an ended session's detail screen, from the session the core
    /// pushed this run or else from its history record.
    func showEnded(_ record: FundingRecord)

    /// Shows the sessions in flight in `direction`, with a way to start
    /// another.
    func showPending(direction: FundingDirection)

    /// Shows every session, in flight and ended.
    func showHistory()
}

/// The runtime's funding overlay. Installed once, before any product
/// execution opens. Keeps what the core pushes in the session book and
/// forwards to whatever draws the overlay.
final class AppFundingHostBridge: FundingHostBridge, @unchecked Sendable {
    private let overlay: FundingOverlayPresenting
    private let book: FundingSessionBook

    init(overlay: FundingOverlayPresenting, book: FundingSessionBook = .shared) {
        self.overlay = overlay
        self.book = book
    }

    func presentFunding(
        productId: String?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async throws -> FundingPresentOutcome {
        let outcome = await overlay.presentFunding(
            productId: productId,
            intent: intent,
            direction: direction,
            amount: amount
        )
        book.forgetOptions(intent: intent)
        return outcome
    }

    func presentProviderFrame(
        providerId: String,
        intent: String,
        target: FundingFrameTarget
    ) async throws -> FundingFrameOutcome {
        await overlay.presentProviderFrame(providerId: providerId, intent: intent, target: target)
    }

    func closeProviderFrame(providerId: String, intent: String) {
        overlay.closeProviderFrame(providerId: providerId, intent: intent)
    }

    func fundingSessionChanged(
        intent: String,
        status: HostFundingStatusSubscribeItem,
        session: FundingSession?,
        progress: FundingProgress?
    ) {
        book.record(intent: intent, session: session, progress: progress)
        overlay.fundingSessionChanged(intent: intent, status: status)
    }

    func fundingInProgressChanged(_ inProgress: Bool) {
        FundingResumeMarker.record(inProgress)
    }

    func fundingOptionsChanged(intent: String, options: FundingOptions) {
        book.record(intent: intent, options: options)
        overlay.fundingOptionsChanged(intent: intent)
    }

    func fundingQuotesChanged(intent: String, quotes: FundingQuoteList) {
        overlay.fundingQuotesChanged(intent: intent, quotes: quotes)
    }

    /// The device's region. The app holds no location permission, and iOS no
    /// longer tells apps the SIM's country, so the region is all it has.
    func detectedCountry() async -> String? {
        FundingCountries.localeRegion()
    }
}
