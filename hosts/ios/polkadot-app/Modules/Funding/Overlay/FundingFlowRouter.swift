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
    /// The session's progress: on its open flow, or the session shown again.
    func showProgress(entryId: String)
}

@MainActor
final class FundingFlowRouter: FundingFlowRouting {
    private let service: FundingServiceProtocol
    private let environment: FundingOverlayEnvironment
    private let tracker: FundingFlowTracking
    private let logger: LoggerProtocol
    private var flows: [String: FundingFlowWireframe] = [:]

    init(
        service: FundingServiceProtocol,
        environment: FundingOverlayEnvironment,
        tracker: FundingFlowTracking,
        logger: LoggerProtocol
    ) {
        self.service = service
        self.environment = environment
        self.tracker = tracker
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

    func showProgress(entryId: String) {
        if let flow = flows[entryId] {
            flow.showProgress()
        } else {
            reopen(entryId: entryId)
        }
    }
}

extension FundingFlowRouter: FundingToastPresentable {}

private extension FundingFlowRouter {
    func show(_ entry: FundingEntry, on screens: [FundingFlowScreen]) {
        let context = FundingFlowContext(
            entry: entry,
            cashInfo: environment.cashInfo,
            branding: environment.branding,
            iconLoader: environment.iconLoader
        )
        let flow = FundingFlowWireframe(
            context: context,
            service: service,
            environment: environment,
            viewModelFactory: makeViewModelFactory()
        )
        flow.onClose = { [weak self] in
            self?.flows[entry.id] = nil
            self?.tracker.flowClosed(entryId: entry.id)
        }
        flow.onHistory = { [weak self] in self?.showHistory() }
        flow.onStartOver = { [weak self] direction in self?.startOver(direction: direction) }
        flows[entry.id] = flow

        guard environment.present(flow.makeSheet(screens)) else {
            flow.close()
            return
        }
        tracker.flowOpened(entryId: entry.id)
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

        let sheet = FundingEndedSheetController(
            ended: ended,
            viewModel: makeViewModelFactory().makeEndedViewModel(for: ended),
            branding: environment.branding
        )
        sheet.onStartOver = { [weak self, weak sheet] in
            sheet?.dismissIfPresented { self?.startOver(direction: entry.direction) }
        }
        sheet.onCopy = { [weak self] in self?.showCopied() }
        if environment.present(sheet) {
            Task { [service] in await service.acknowledge(entry.id) }
        }
    }

    func showList(_ mode: FundingActivityMode) {
        let sheet = FundingActivityViewFactory.createView(
            mode: mode,
            viewModelFactory: makeViewModelFactory(),
            service: service,
            router: self
        )
        _ = environment.present(sheet)
    }

    func makeViewModelFactory() -> FundingViewModelFactoryProtocol {
        FundingViewModelFactory(cashInfo: environment.cashInfo, cashSymbol: environment.cashSymbol)
    }

    func current(_ entryId: String) async -> FundingEntry? {
        var states = service.subscribeEntryState(entryId).makeAsyncIterator()
        return try? await states.next()
    }
}
