import Foundation
import TrUAPIHost
import UIKit

/// What the funding flows need from the app around them.
@MainActor
protocol FundingOverlayEnvironment: FundingFlowEnvironment {
    /// Presents over whatever is on screen; false when nothing is.
    func present(_ controller: UIViewController) -> Bool
    /// The provider product's page at `route`, as the app shows products.
    func providerPage(providerId: ProductId, route: String, title: String) -> UIViewController?
    /// A passing error notice over whatever is on screen.
    func showNotice(_ message: String)
}

/// Opens the funding screens: a session's flow, the session lists, and an
/// ended session's detail.
@MainActor
protocol FundingFlowRouting: AnyObject {
    /// Shows the flow for a session the core opened.
    func present(_ entry: FundingEntry)
    /// Shows a session again: one in flight on its progress screen, an ended
    /// one on its detail screen.
    func reopen(entryId: String)
    func showPending(direction: FundingDirection)
    func showHistory()
    /// Opens a new session in `direction`.
    func startOver(direction: FundingDirection)
    /// Whether session `entryId`'s flow is on screen.
    func isShowing(entryId: String) -> Bool
    /// The session's progress: on its open flow, or the session shown again.
    func showProgress(entryId: String)
}

@MainActor
final class FundingFlowRouter: FundingFlowRouting {
    private let service: FundingServiceProtocol
    private let environment: FundingOverlayEnvironment
    private let logger: LoggerProtocol
    private var flows: [String: FundingFlowWireframe] = [:]

    init(service: FundingServiceProtocol, environment: FundingOverlayEnvironment, logger: LoggerProtocol) {
        self.service = service
        self.environment = environment
        self.logger = logger
    }

    func present(_ entry: FundingEntry) {
        show(entry, on: [.amount])
    }

    func reopen(entryId: String) {
        guard flows[entryId] == nil else { return }

        Task {
            guard let entry = await current(entryId) else { return }
            if entry.hasEnded {
                showEnded(entry)
            } else if entry.hasStarted {
                show(entry, on: resumedScreens(of: entry))
            }
        }
    }

    func showPending(direction: FundingDirection) {
        showList(.pending(direction))
    }

    func showHistory() {
        showList(.history)
    }

    func startOver(direction: FundingDirection) {
        Task { [service, logger] in
            do {
                try await service.open(direction, amount: nil)
            } catch {
                logger.error("[funding] starting over failed: \(error)")
            }
        }
    }

    func isShowing(entryId: String) -> Bool {
        flows[entryId]?.navigation?.presentingViewController != nil
    }

    func showProgress(entryId: String) {
        if let flow = flows[entryId] {
            flow.showProgress()
        } else {
            reopen(entryId: entryId)
        }
    }
}

private extension FundingFlowRouter {
    func show(_ entry: FundingEntry, on screens: [FundingFlowScreen]) {
        let context = FundingFlowContext(entry: entry, cash: environment.cash, branding: environment.branding)
        let flow = FundingFlowWireframe(context: context, service: service, environment: environment)
        flow.onClose = { [weak self] in self?.flows[entry.id] = nil }
        flow.onHistory = { [weak self] in self?.showHistory() }
        flow.onStartOver = { [weak self] direction in self?.startOver(direction: direction) }
        flows[entry.id] = flow

        if !environment.present(flow.makeSheet(screens)) {
            flow.close()
        }
    }

    /// A session left running shows its progress, with the crypto deposit
    /// over it while the funds have not been seen.
    func resumedScreens(of entry: FundingEntry) -> [FundingFlowScreen] {
        let paysByDeposit = entry.direction == .in && entry.session?.choice?.rail == .crypto
        return paysByDeposit && !entry.hasReached(.received) ? [.progress, .deposit] : [.progress]
    }

    /// An ended session, drawn from its summary in the core's history.
    func showEnded(_ entry: FundingEntry) {
        guard let ended = entry.ended else { return }

        let sheet = FundingEndedSheetController(ended: ended, cash: environment.cash, branding: environment.branding)
        sheet.onStartOver = { [weak self, weak sheet] in
            sheet?.dismissIfPresented { self?.startOver(direction: entry.direction) }
        }
        if environment.present(sheet) {
            Task { [service] in await service.acknowledge(entry.id) }
        }
    }

    func showList(_ mode: FundingActivityMode) {
        let sheet = FundingActivityViewFactory.createView(
            mode: mode,
            cash: environment.cash,
            service: service,
            router: self
        )
        _ = environment.present(sheet)
    }

    func current(_ entryId: String) async -> FundingEntry? {
        var states = service.subscribeEntryState(entryId).makeAsyncIterator()
        return try? await states.next()
    }
}
