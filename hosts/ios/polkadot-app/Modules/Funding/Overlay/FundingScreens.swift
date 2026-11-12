import Foundation
import TrUAPIHost

/// The funding screens the main tab bar shows for the core: a session's
/// flow, the providers' own screens, the notice that a provider asks for one,
/// and the toast that a provider is not working.
@MainActor
final class FundingScreens {
    let router: FundingFlowRouter
    private let frames: FundingFramePresenter
    private let notice: FundingFrameNoticePresenting
    private let environment: FundingOverlayEnvironment
    /// The session whose provider's notice is on screen, and how to answer
    /// the provider when the notice goes unanswered.
    private var pendingNotice: (entryId: String, dismiss: () -> Void)?

    init(
        service: FundingServiceProtocol,
        environment: FundingOverlayEnvironment,
        notice: FundingFrameNoticePresenting,
        logger: LoggerProtocol
    ) {
        router = FundingFlowRouter(service: service, environment: environment, logger: logger)
        frames = FundingFramePresenter(service: service, environment: environment)
        self.notice = notice
        self.environment = environment
    }

    func present(_ entry: FundingEntry) {
        router.present(entry)
    }

    func isShowing(entryId: String) -> Bool {
        router.isShowing(entryId: entryId)
    }

    func present(_ frame: FundingFrame) {
        Task { await frames.present(frame) }
    }

    /// Takes down the provider's screens for session `entryId`, or its notice,
    /// and shows the session in their place when a screen was up.
    func closeFrames(entryId: String) {
        if pendingNotice?.entryId == entryId {
            pendingNotice = nil
            notice.hide()
        }
        frames.close(entryId: entryId) { [router] in
            router.showProgress(entryId: entryId)
        }
    }

    /// Tells the user the provider of `request` wants to show a screen. A
    /// notice already up is replaced, and its provider answered as dismissed.
    func showFrameNotice(
        for request: FundingFrameRequest,
        onShow: @escaping () -> Void,
        onDismiss: @escaping () -> Void
    ) {
        Task {
            let brand = await environment.branding.brand(for: request.providerId)
            let replaced = pendingNotice
            pendingNotice = (request.entryId, onDismiss)
            replaced?.dismiss()
            notice.show(
                brand: brand,
                onShow: { [weak self] in
                    self?.pendingNotice = nil
                    onShow()
                },
                onDismiss: { [weak self] in
                    self?.pendingNotice = nil
                    onDismiss()
                }
            )
        }
    }

    func showProviderNotWorking(_ providerId: ProductId) {
        Task {
            let brand = await environment.branding.brand(for: providerId)
            environment.showNotice(String(localized: .Funding.providerCrashing(provider: brand.name)))
        }
    }
}
