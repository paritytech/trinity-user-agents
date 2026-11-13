import Foundation
import TrUAPIHost
import UIKit

/// Shows the providers' own screens the core presents: an App route in the
/// provider's frame, or an external page in one the host owns.
@MainActor
final class FundingFramePresenter {
    private let service: FundingServiceProtocol
    private let environment: FundingOverlayEnvironment
    private var frames: [String: FundingProviderFrameController] = [:]
    private var pages: [String: FundingWebPageController] = [:]

    init(service: FundingServiceProtocol, environment: FundingOverlayEnvironment) {
        self.service = service
        self.environment = environment
    }

    /// Shows `frame`, answering the core once it is gone.
    func present(_ frame: FundingFrame) async {
        switch frame.target {
        case let .app(route):
            await presentApp(frame, route: route)
        case let .page(url, returnUrl):
            presentPage(frame, url: url, returnUrl: returnUrl)
        }
    }

    /// The provider is done with its screens for session `entryId`: takes
    /// them down, then calls `shown` when one was on screen.
    func close(entryId: String, shown: @escaping () -> Void) {
        if let frame = frames.removeValue(forKey: entryId) {
            let wasShown = frame.presentingViewController != nil
            frame.finish(.closed, dismissed: { if wasShown { shown() } })
        }
        if let page = pages.removeValue(forKey: entryId) {
            let wasShown = page.presentingViewController != nil
            page.finish(.closed, dismissed: { if wasShown { shown() } })
        }
    }
}

private extension FundingFramePresenter {
    func presentApp(_ frame: FundingFrame, route: String) async {
        let brand = await environment.branding.brand(for: frame.providerId)
        guard let page = environment.providerPage(providerId: frame.providerId, route: route, title: brand.name) else {
            service.answer(frame: frame.id, with: .dismissed)
            return
        }

        let controller = FundingProviderFrameController(
            providerId: frame.providerId,
            page: page,
            showsSentFunds: frame.awaitsUserTransfer
        ) { [weak self] outcome in
            self?.finished(frame, with: outcome)
        }
        frames[frame.entryId] = controller
        if !environment.present(controller) {
            controller.finish(.dismissed)
        }
    }

    func presentPage(_ frame: FundingFrame, url: String, returnUrl: String?) {
        guard let pageURL = URL(string: url) else {
            service.answer(frame: frame.id, with: .dismissed)
            return
        }

        let controller = FundingWebPageController(
            providerId: frame.providerId,
            url: pageURL,
            returnURL: returnUrl.flatMap(URL.init(string:))
        ) { [weak self] outcome in
            self?.finished(frame, with: outcome)
        }
        pages[frame.entryId] = controller
        if !environment.present(controller) {
            controller.finish(.dismissed)
        }
    }

    func finished(_ frame: FundingFrame, with outcome: FundingFrameOutcome) {
        if frames[frame.entryId]?.providerId == frame.providerId { frames[frame.entryId] = nil }
        if pages[frame.entryId]?.providerId == frame.providerId { pages[frame.entryId] = nil }
        service.answer(frame: frame.id, with: outcome)
    }
}
