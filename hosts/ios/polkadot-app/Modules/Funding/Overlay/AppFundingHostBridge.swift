import Foundation
import TrUAPIHost

/// What the core asks of the app's funding side: the flow a session is
/// started from, the frame a provider's own screens are shown in, and the
/// notices that something a session draws changed.
protocol FundingCoreEventHandling: AnyObject, Sendable {
    func presentFunding(intent: String, direction: FundingDirection, amount: U128?) async -> FundingPresentOutcome

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget
    ) async -> FundingFrameOutcome

    /// The provider closed its frame for session `intent`.
    func closeProviderFrame(providerId: ProductId, intent: String)

    /// Something session `intent` draws changed in the core.
    func fundingChanged(intent: String, kind: FundingChangeKind)

    /// Product `productId`'s worker keeps crashing while the core restarts it.
    func workerCrashing(productId: ProductId)
}

/// The runtime's funding overlay. Installed once, before any product
/// execution opens, and hands the core's calls to the funding service.
final class AppFundingHostBridge: FundingHostBridge, Sendable {
    private let handler: FundingCoreEventHandling

    init(handler: FundingCoreEventHandling) {
        self.handler = handler
    }

    func presentFunding(
        productId _: ProductId?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async throws -> FundingPresentOutcome {
        await handler.presentFunding(intent: intent, direction: direction, amount: amount)
    }

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget
    ) async throws -> FundingFrameOutcome {
        await handler.presentProviderFrame(providerId: providerId, intent: intent, target: target)
    }

    func closeProviderFrame(providerId: ProductId, intent: String) {
        handler.closeProviderFrame(providerId: providerId, intent: intent)
    }

    /// A change with no session, such as the providers on offer, needs
    /// nothing here: every screen reads the providers through a session.
    func fundingChanged(intent: String?, kind: FundingChangeKind) {
        guard let intent else { return }

        handler.fundingChanged(intent: intent, kind: kind)
    }

    /// The device's region. The app holds no location permission, and iOS no
    /// longer tells apps the SIM's country, so the region is all it has.
    func detectedCountry() async -> String? {
        FundingCountries.localeRegion()
    }
}
