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

    func presentProviderFrame(providerId: String, intent: String, route: String) async -> FundingFrameOutcome

    func fundingSessionChanged(intent: String, status: HostFundingStatusSubscribeItem)

    func fundingQuoteChanged(intent: String, row: FundingQuoteRow)
}

/// Stands in until the overlay UI lands: every session is dismissed, so the
/// core discards it, and every provider frame reports the user closed it.
final class PendingFundingOverlay: FundingOverlayPresenting, @unchecked Sendable {
    private let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }

    func presentFunding(
        productId: String?,
        intent: String,
        direction: FundingDirection,
        amount _: U128?
    ) async -> FundingPresentOutcome {
        let opener = productId ?? "host"
        logger.warning("Funding overlay not built yet, dismissing \(direction) session \(intent) for \(opener)")
        return .dismissed
    }

    func presentProviderFrame(providerId: String, intent: String, route _: String) async -> FundingFrameOutcome {
        logger.warning("Funding overlay not built yet, dismissing \(providerId) frame for session \(intent)")
        return .dismissed
    }

    func fundingSessionChanged(intent _: String, status _: HostFundingStatusSubscribeItem) {}

    func fundingQuoteChanged(intent _: String, row _: FundingQuoteRow) {}
}

/// The runtime's funding overlay. Installed once, before any product
/// execution opens, and forwards to whatever draws the overlay.
final class AppFundingHostBridge: FundingHostBridge, @unchecked Sendable {
    private let overlay: FundingOverlayPresenting

    init(overlay: FundingOverlayPresenting) {
        self.overlay = overlay
    }

    func presentFunding(
        productId: String?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async throws -> FundingPresentOutcome {
        await overlay.presentFunding(productId: productId, intent: intent, direction: direction, amount: amount)
    }

    func presentProviderFrame(providerId: String, intent: String, route: String) async throws -> FundingFrameOutcome {
        await overlay.presentProviderFrame(providerId: providerId, intent: intent, route: route)
    }

    func fundingSessionChanged(intent: String, status: HostFundingStatusSubscribeItem) {
        overlay.fundingSessionChanged(intent: intent, status: status)
    }

    func fundingQuoteChanged(intent: String, row: FundingQuoteRow) {
        overlay.fundingQuoteChanged(intent: intent, row: row)
    }
}
