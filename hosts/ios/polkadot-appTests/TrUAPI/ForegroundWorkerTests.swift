import Foundation
import os
import Products
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A product that declares a foreground worker runs while the app is in the foreground (VERBOSITY-WASM-PRODUCT I5).
/// The core decides: the host reports the state of the app and the declaration of the product.
struct ForegroundWorkerTests {
    // MARK: - The state of the app

    /// The app is in the foreground from `willEnterForeground` to `didEnterBackground`. A resign of active alone
    /// (a system alert, Face ID) keeps it in the foreground.
    @Test func theMonitorFollowsTheForegroundAndTheBackground() {
        let center = NotificationCenter()
        let monitor = AppForegroundMonitor(isForeground: true, notificationCenter: center)
        let changes = OSAllocatedUnfairLock(initialState: [Bool]())
        monitor.onChange { value in changes.withLock { $0.append(value) } }

        center.post(name: UIApplication.willResignActiveNotification, object: nil)
        #expect(monitor.isForeground)

        center.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        center.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        #expect(!monitor.isForeground)

        center.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        #expect(monitor.isForeground)
        #expect(changes.withLock { $0 } == [false, true])
    }

    // MARK: - The reports to the core

    /// The core starts in the background, so the reporter sends the state once the runtime exists, and sends
    /// nothing before.
    @Test func theReporterSendsTheStateOnceTheRuntimeExists() {
        let center = NotificationCenter()
        let monitor = AppForegroundMonitor(isForeground: true, notificationCenter: center)
        let core = RecordingCore()
        let workers = StubWorkerManager()
        let built = OSAllocatedUnfairLock(initialState: false)
        let reporter = TrUAPIForegroundReporter(
            monitor: monitor,
            runtime: { built.withLock { $0 } ? core : nil },
            workers: { workers }
        )

        reporter.report()
        reporter.waitForReports()
        #expect(core.reports.isEmpty)

        built.withLock { $0 = true }
        reporter.report()
        reporter.waitForReports()
        #expect(core.reports == [true])
        #expect(workers.foregroundReports == [true])
    }

    /// Each change reaches the core once, and a repeated report sends nothing more.
    @Test func theReporterSendsEachChangeOnce() {
        let center = NotificationCenter()
        let monitor = AppForegroundMonitor(isForeground: true, notificationCenter: center)
        let core = RecordingCore()
        let workers = StubWorkerManager()
        let reporter = TrUAPIForegroundReporter(monitor: monitor, runtime: { core }, workers: { workers })

        reporter.report()
        reporter.waitForReports()
        center.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        reporter.waitForReports()
        reporter.report()
        reporter.waitForReports()
        center.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        reporter.waitForReports()

        #expect(core.reports == [true, false, true])
        #expect(workers.foregroundReports == [true, false, true])
    }

    // MARK: - The declaration of the product

    /// The core holds one flag for each product, so the manager tells it only when the first holder declares a
    /// foreground worker and when the last one withdraws it.
    @Test func theManagerTellsTheCoreOnlyAtTheFirstDeclarationAndTheLastWithdrawal() {
        let references = StubWorkerReferences()
        let manager = TrUAPIWorkerManager(
            builder: NoWorkerBuilder(),
            collection: InMemoryPocketCardStore([]),
            references: { references }
        )

        manager.declareForegroundWorker(for: "chat.dot")
        manager.declareForegroundWorker(for: "chat.dot")
        manager.withdrawForegroundWorker(for: "chat.dot")
        #expect(references.log == ["keep alive chat.dot foreground"])

        manager.withdrawForegroundWorker(for: "chat.dot")
        manager.withdrawForegroundWorker(for: "chat.dot")
        #expect(references.log == ["keep alive chat.dot foreground", "keep alive chat.dot onDemand"])
    }

    /// A product that declares a foreground worker holds it from the start of its chat session to the dispose.
    @Test func aForegroundProductDeclaresItsWorkerForItsSession() async throws {
        let workers = StubWorkerManager()
        let handler = TrUAPIChatHandler(
            productId: "chat.dot",
            workers: workers,
            wakesAtStart: false,
            keepAlive: .foreground
        )

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        #expect(workers.foregroundDeclarations == ["declare chat.dot"])
        #expect(workers.references.acquired.isEmpty, "the core takes the hold, not the handler")

        await handler.dispose()
        await handler.dispose()
        #expect(workers.foregroundDeclarations == ["declare chat.dot", "withdraw chat.dot"])
    }

    @Test func aProductOnDemandDeclaresNothing() async throws {
        let workers = StubWorkerManager()
        let handler = TrUAPIChatHandler(productId: "chat.dot", workers: workers, wakesAtStart: false)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        await handler.dispose()

        #expect(workers.foregroundDeclarations.isEmpty)
    }

    @Test func aDisposeBeforeTheStartDeclaresNothing() async throws {
        let workers = StubWorkerManager()
        let handler = TrUAPIChatHandler(
            productId: "chat.dot",
            workers: workers,
            wakesAtStart: false,
            keepAlive: .foreground
        )

        await handler.dispose()

        #expect(workers.foregroundDeclarations.isEmpty)
    }
}

/// A builder for a manager whose tests start no worker.
private struct NoWorkerBuilder: TrUAPIWorkerBuilding {
    func makeRuntime(
        productId _: ProductId,
        context _: ProductWorkerContext,
        pocket _: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime {
        throw CancellationError()
    }
}

/// A core that records the reports of the app state.
private final class RecordingCore: TrUAPIAppForegroundReceiving, @unchecked Sendable {
    private let records = OSAllocatedUnfairLock(initialState: [Bool]())

    var reports: [Bool] { records.withLock { $0 } }

    func setAppForeground(foreground: Bool) {
        records.withLock { $0.append(foreground) }
    }
}
