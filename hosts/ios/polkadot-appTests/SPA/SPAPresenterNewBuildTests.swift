import Foundation
import FoundationExt
import Operation_iOS
import Products
import Testing
import UIKit
@testable import UIKitExt
@testable import polkadot_app

/// The new-build prompt from a screen becoming visible, through the interactor and runtime, to the alert and the
/// Reload or Later choice the user makes on it.
@MainActor
struct SPAPresenterNewBuildTests {
    @Test func aNewBuildOffersOneAlertNamedAfterTheProduct() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }

        #expect(screen.wireframe.presentedAlerts.count == 1)
        #expect(screen.wireframe.presentedAlerts.first?.title?.contains("Test Product") == true)
    }

    @Test func aVisitWhileTheAlertIsOnScreenDoesNotOfferItAgain() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }
        screen.presenter.didBecomeVisible()
        await waitUntil { runtime.completedNewBuildCheckCount == 2 }
        await settle()

        #expect(screen.wireframe.presentedAlerts.count == 1)
    }

    @Test func visitsWhileACheckIsInFlightAskTheRuntimeOnce() async throws {
        let (gate, release) = AsyncStream.makeStream(of: Void.self)
        let runtime = try makeRuntime(newBuildResult: true, newBuildGate: gate)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { runtime.newBuildRequestCount == 1 }
        screen.presenter.didBecomeVisible()
        await settle()
        release.finish()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }

        #expect(runtime.newBuildRequestCount == 1)
    }

    @Test func reloadUsesTheLatestBuildAndReloadsTheView() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }
        let reload = try #require(screen.wireframe.presentedAlerts.first?.actions.first)
        reload.handler?()
        await waitUntil { screen.view.reloadCount > 0 }

        #expect(runtime.latestBuildRequestCount == 1)
        #expect(screen.view.reloadCount == 1)
    }

    @Test func laterDismissesWithoutReloadingAndTheNextVisitAsksAgain() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }
        let later = try #require(screen.wireframe.presentedAlerts.first?.actions.last)
        later.handler?()
        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 2 }

        #expect(runtime.latestBuildRequestCount == 0)
        #expect(screen.view.reloadCount == 0)
    }

    @Test func aVisitWhileTheScreenIsOffScreenOffersNothingUntilItIsOnScreen() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        screen.view.isOnScreen = false
        screen.presenter.didBecomeVisible()
        await waitUntil { runtime.completedNewBuildCheckCount == 1 }
        await settle()
        #expect(screen.wireframe.presentedAlerts.isEmpty)

        screen.view.isOnScreen = true
        screen.presenter.didBecomeVisible()
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }

        #expect(screen.wireframe.presentedAlerts.count == 1)
    }

    @Test func enteringForegroundChecksForANewBuildAndOffersIt() async throws {
        let runtime = try makeRuntime(newBuildResult: true)
        let screen = try await makeScreen(runtime: runtime)

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await waitUntil { screen.wireframe.presentedAlerts.count == 1 }

        #expect(runtime.newBuildRequestCount == 1)
        #expect(screen.wireframe.presentedAlerts.count == 1)
    }

    @Test func noNewBuildPresentsNoAlert() async throws {
        let runtime = try makeRuntime(newBuildResult: false)
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        await waitUntil { runtime.completedNewBuildCheckCount == 1 }
        await settle()

        #expect(screen.wireframe.presentedAlerts.isEmpty)
    }
}

private struct Screen {
    let presenter: SPAPresenter
    let view: StubSPAView
    let wireframe: StubSPAWireframe
}

private func makeRuntime(
    newBuildResult: Bool = false,
    newBuildGate: AsyncStream<Void>? = nil
) throws -> MockSPARuntime {
    try MockSPARuntime(
        startURL: #require(URL(string: "https://test.dot")),
        newBuildResult: newBuildResult,
        newBuildGate: newBuildGate
    )
}

@MainActor
private func makeScreen(runtime: MockSPARuntime) async throws -> Screen {
    let factory = ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))
    let host = try #require(factory.host(rawString: "test.dot"))
    let configuration = SPAConfiguration(
        title: nil,
        isRootScreen: false,
        showMoreButton: false,
        page: ProductPage(host: host)
    )
    let product = ResolvedProduct.legacy(id: "test.dot", displayName: "Test Product")
    let interactor = SPARustRuntimeInteractor(
        runtimeFactory: MockSPARuntimeFactory(runtime: runtime),
        productResolver: StubProductResolver(alwaysResolvingTo: product),
        configuration: configuration,
        logger: Logger.shared,
        productRepository: AnyDataProviderRepository(InMemoryDataProviderRepository<Product>()),
        chatProviderFactory: MockChatContactDataProviderFactory(),
        applicationStateStreams: ApplicationStateStreamFactory()
    )
    let wireframe = StubSPAWireframe()
    let presenter = SPAPresenter(
        interactor: interactor,
        wireframe: wireframe,
        configuration: configuration,
        hostProvider: StubProductHostProvider(syncHostResult: nil, asyncHostResult: .success(nil))
    )
    let view = StubSPAView()
    presenter.view = view
    interactor.presenter = presenter
    presenter.setup(engine: MockJSEngine())
    // The product name comes from the setup task, which finishes before the runtime starts.
    await waitUntil { runtime.startCount == 1 }

    return Screen(presenter: presenter, view: view, wireframe: wireframe)
}

/// Gives a check that has already finished time to deliver its result to the presenter.
@MainActor
private func settle() async {
    for _ in 0 ..< 100 {
        await Task.yield()
    }
}
