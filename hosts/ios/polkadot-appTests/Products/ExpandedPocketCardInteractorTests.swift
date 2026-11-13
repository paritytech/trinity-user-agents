import Foundation
import IssueMonitoring
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
struct ExpandedPocketCardInteractorTests {
    @Test func startsTheRuntimeUnderTheInteractorAsTheFace() async throws {
        let environment = try Environment()

        try await environment.start()

        #expect(environment.output.navigations.map(\.host) == ["test.dot"])
        #expect(environment.runtimes.faces.count == 1)
        #expect(environment.runtimes.faces.first === environment.interactor)
    }

    @Test(arguments: [(true, "widget.test.dot"), (false, "test.dot")])
    func registersPresenceForTheServedExecutable(_ publishesWidget: Bool, contentId: String) async throws {
        let environment = try Environment(product: publishesWidget ? productPublishingAWidget() : nil)

        try await environment.start()

        #expect(try environment.registry.registrations.map(\.contentId) == [ProductId(contentId)])
    }

    @Test func retryKeepsThePresence() async throws {
        let environment = try Environment()
        try await environment.start()

        environment.interactor.retry()
        await environment.output.waitForNavigations(2)

        #expect(environment.registry.registrations.count == 1)
        #expect(environment.registry.unregisteredIds.isEmpty)
    }

    @Test func servesTheAppliedBuildAndReloadsThePage() async throws {
        let environment = try Environment()
        try await environment.start()

        environment.runtimes.loader.simulate(appliedContentHash: "01")
        environment.interactor.serveAppliedBuild()
        await environment.output.waitForReloads(1)

        #expect(environment.interactor.servedContentHash == "01")
        #expect(environment.registry.registrations.count == 1)
    }

    @Test func checksForANewBuildOnlyOnceServing() async throws {
        let environment = try Environment()
        environment.interactor.checkForNewBuild()

        try await environment.start()
        environment.interactor.checkForNewBuild()

        #expect(try environment.registry.refreshedProductIds == [ProductId("test.dot")])
    }

    @Test func faceRequestsAnswerWithThePresentersOutcome() throws {
        let environment = try Environment()
        environment.output.faceOutcome = .userMoving

        #expect(environment.interactor.setFaceShown(false) == .userMoving)
        #expect(environment.output.faceRequests == [false])
    }
}

@MainActor
private struct Environment {
    let interactor: ExpandedPocketCardInteractor
    let runtimes: RecordingRuntimeFactory
    let registry = RecordingPresenceRegistry()
    let output = RecordingOutput()

    init(product: ResolvedProduct? = nil) throws {
        let configuration = try makeConfiguration()
        runtimes = RecordingRuntimeFactory(
            configuration: configuration,
            loader: StubProductContentLoader(product: product)
        )
        interactor = ExpandedPocketCardInteractor(
            runtimeFactory: runtimes,
            presenceRegistry: registry,
            configuration: configuration,
            logger: Logger.shared
        )
        interactor.presenter = output
    }

    func start() async throws {
        interactor.setup(engine: MockJSEngine())
        await output.waitForNavigations(1)
    }
}

@MainActor
private final class RecordingRuntimeFactory: ExpandedPocketCardRuntimeMaking {
    let configuration: SPAConfiguration
    let loader: StubProductContentLoader
    private(set) var faces: [any ExpandedCardFaceShowing] = []

    init(configuration: SPAConfiguration, loader: StubProductContentLoader) {
        self.configuration = configuration
        self.loader = loader
    }

    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol {
        faces.append(face)

        return SPARustRuntime(
            executionModel: makeExecutionModel(),
            configuration: configuration,
            productOpening: SPARustRuntime.ProductOpening(
                loader: loader,
                strategy: .withAppFallback(.widget),
                resolver: StubProductResolver()
            ),
            schemeHandlerProxy: SchemeHandlerProxy(),
            logger: Logger.shared,
            hostEventRecorder: NoopHostEventRecorder()
        )
    }
}

@MainActor
private final class RecordingOutput: ExpandedPocketCardInteractorOutputProtocol {
    private(set) var navigations: [URL] = []
    private(set) var faceRequests: [Bool] = []
    var faceOutcome: ExpandedCardFaceOutcome = .applied
    private(set) var reloads = 0
    private var navigationWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    private var reloadWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []

    func waitForNavigations(_ count: Int) async {
        guard navigations.count < count else { return }

        await withCheckedContinuation { navigationWaiters.append((count, $0)) }
    }

    func waitForReloads(_ count: Int) async {
        guard reloads < count else { return }

        await withCheckedContinuation { reloadWaiters.append((count, $0)) }
    }

    func didRequestNavigation(to url: URL) {
        navigations.append(url)
        navigationWaiters = Self.resume(navigationWaiters, reaching: navigations.count)
    }

    func didRequestReload() {
        reloads += 1
        reloadWaiters = Self.resume(reloadWaiters, reaching: reloads)
    }

    func didUpdateLoadPhase(_: ProductLoadPhase) {}

    func didFail(error _: Error) {}

    func didRequestFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        faceRequests.append(shown)
        return faceOutcome
    }

    private static func resume(
        _ waiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)],
        reaching count: Int
    ) -> [(count: Int, continuation: CheckedContinuation<Void, Never>)] {
        waiters.filter { $0.count <= count }.forEach { $0.continuation.resume() }
        return waiters.filter { $0.count > count }
    }
}

private func makeConfiguration() throws -> SPAConfiguration {
    let hosts = ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))
    let host = try #require(hosts.host(rawString: "test.dot"))

    return SPAConfiguration(title: nil, isRootScreen: false, showMoreButton: false, page: ProductPage(host: host))
}

private func productPublishingAWidget() throws -> ResolvedProduct {
    try ResolvedProduct(
        id: ProductId("test.dot"),
        displayName: "Test",
        description: nil,
        icon: nil,
        executables: ProductExecutables(
            app: ProductExecutable.App(identifier: ProductId("app.test.dot"), appVersion: .zero),
            widget: ProductExecutable.Widget(
                identifier: ProductId("widget.test.dot"),
                appVersion: .zero,
                description: nil,
                heights: [2],
                width: 1
            ),
            worker: nil
        ),
        hasManifest: true
    )
}
