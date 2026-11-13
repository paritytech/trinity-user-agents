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
    /// The session whose provider's notice is on screen.
    private var noticeEntryId: String?

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
        if noticeEntryId == entryId {
            noticeEntryId = nil
            notice.hide()
        }
        frames.close(entryId: entryId) { [router] in
            router.showProgress(entryId: entryId)
        }
    }

    /// Tells the user the provider of `request` wants to show a screen, in
    /// place of any notice already up. The screen keeps waiting in the core
    /// when the notice is closed.
    func showFrameNotice(for request: FundingFrameRequest, onShow: @escaping () -> Void) {
        Task {
            let brand = await environment.branding.brand(for: request.providerId)
            noticeEntryId = request.entryId
            notice.show(
                brand: brand,
                onShow: { [weak self] in
                    self?.noticeEntryId = nil
                    onShow()
                },
                onDismiss: { [weak self] in self?.noticeEntryId = nil }
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
