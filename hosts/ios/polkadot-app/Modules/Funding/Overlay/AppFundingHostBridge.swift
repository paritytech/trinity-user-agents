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
        productId: ProductId?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async -> FundingPresentOutcome

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget,
        context: FundingFrameContext
    ) async -> FundingFrameOutcome

    /// The provider closed its frame for session `intent`: answer it
    /// `.closed` and show the session sheet in its place.
    func closeProviderFrame(providerId: ProductId, intent: String)

    /// Whether provider `providerId` ships an App its frame can show.
    func providerHasApp(providerId: ProductId) async -> Bool

    /// Session `intent` changed in the core: read it back from the runtime.
    func fundingSessionChanged(intent: String)

    /// What session `intent`'s overlay offers changed in the core.
    func fundingOptionsChanged(intent: String)

    /// Session `intent`'s quote list changed in the core.
    func fundingQuotesChanged(intent: String)

    /// Product `productId`'s worker keeps crashing while the core restarts it.
    func workerCrashing(productId: ProductId)

    /// Shows session `intent` again: one the user left running, or an ended
    /// one on its detail screen.
    func reopen(intent: String)

    /// Shows the sessions in flight in `direction`, with a way to start
    /// another.
    func showPending(direction: FundingDirection)

    /// Shows every session, in flight and ended.
    func showHistory()
}

/// The runtime's funding overlay. Installed once, before any product
/// execution opens. Forwards the core's calls and change notices to whatever
/// draws the overlay, which reads what changed back from the runtime.
final class AppFundingHostBridge: FundingHostBridge, Sendable {
    private let overlay: FundingOverlayPresenting

    init(overlay: FundingOverlayPresenting) {
        self.overlay = overlay
    }

    func presentFunding(
        productId: ProductId?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async throws -> FundingPresentOutcome {
        await overlay.presentFunding(
            productId: productId,
            intent: intent,
            direction: direction,
            amount: amount
        )
    }

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget,
        context: FundingFrameContext
    ) async throws -> FundingFrameOutcome {
        await overlay.presentProviderFrame(providerId: providerId, intent: intent, target: target, context: context)
    }

    func closeProviderFrame(providerId: ProductId, intent: String) {
        overlay.closeProviderFrame(providerId: providerId, intent: intent)
    }

    func providerHasApp(providerId: ProductId) async -> Bool {
        await overlay.providerHasApp(providerId: providerId)
    }

    /// Whether a provider still works on a session is kept by the core, which
    /// answers `fundingResumePending` at the next launch, so `.inProgress`
    /// needs nothing here.
    func fundingChanged(intent: String?, kind: FundingChangeKind) {
        guard let intent else { return }

        switch kind {
        case .session: overlay.fundingSessionChanged(intent: intent)
        case .options: overlay.fundingOptionsChanged(intent: intent)
        case .quotes: overlay.fundingQuotesChanged(intent: intent)
        case .inProgress: break
        }
    }

    /// The device's region. The app holds no location permission, and iOS no
    /// longer tells apps the SIM's country, so the region is all it has.
    func detectedCountry() async -> String? {
        FundingCountries.localeRegion()
    }
}
