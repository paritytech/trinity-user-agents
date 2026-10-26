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
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        try await screen.wireframe.alertPresented.next()

        #expect(screen.wireframe.presentedAlerts.count == 1)
        #expect(screen.wireframe.presentedAlerts.first?.title?.contains("Test Product") == true)
    }

    @Test func aVisitWhileTheAlertIsOnScreenDoesNotOfferItAgain() async throws {
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didFindNewBuild(productName: "Test Product")
        screen.presenter.didFindNewBuild(productName: "Test Product")

        #expect(screen.wireframe.presentedAlerts.count == 1)
    }

    @Test func reloadUsesTheLatestBuildAndReloadsTheView() async throws {
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        try await screen.wireframe.alertPresented.next()
        let reload = try #require(screen.wireframe.presentedAlerts.first?.actions.first)
        reload.handler?()
        try await screen.view.reloaded.next()

        #expect(runtime.latestBuildRequestCount == 1)
        #expect(screen.view.reloadCount == 1)
    }

    @Test func laterDismissesWithoutReloadingAndTheNextVisitAsksAgain() async throws {
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        screen.presenter.didBecomeVisible()
        try await screen.wireframe.alertPresented.next()
        let later = try #require(screen.wireframe.presentedAlerts.first?.actions.last)
        later.handler?()
        screen.presenter.didBecomeVisible()
        try await screen.wireframe.alertPresented.next()

        #expect(runtime.latestBuildRequestCount == 0)
        #expect(screen.view.reloadCount == 0)
    }

    @Test func aVisitWhileTheScreenIsOffScreenOffersNothingUntilItIsOnScreen() async throws {
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        screen.view.isOnScreen = false
        screen.presenter.didFindNewBuild(productName: "Test Product")
        #expect(screen.wireframe.presentedAlerts.isEmpty)

        screen.view.isOnScreen = true
        screen.presenter.didFindNewBuild(productName: "Test Product")
        #expect(screen.wireframe.presentedAlerts.count == 1)
    }

    @Test func enteringForegroundChecksForANewBuildAndOffersIt() async throws {
        let runtime = try makeRuntime()
        let screen = try await makeScreen(runtime: runtime)

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        try await screen.wireframe.alertPresented.next()

        #expect(runtime.newBuildRequestCount == 1)
        #expect(screen.wireframe.presentedAlerts.count == 1)
    }
}

private struct Screen {
    let presenter: SPAPresenter
    let view: StubSPAView
    let wireframe: StubSPAWireframe
}

private func makeRuntime() throws -> MockSPARuntime {
    try MockSPARuntime(startURL: #require(URL(string: "https://test.dot")))
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
    try await runtime.started.next()

    return Screen(presenter: presenter, view: view, wireframe: wireframe)
}
