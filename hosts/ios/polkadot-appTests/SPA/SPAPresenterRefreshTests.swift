import Foundation
import FoundationExt
import Operation_iOS
import Products
import Testing
@testable import polkadot_app

/// Refresh from the presenter through the interactor and runtime back to the web view, whether or not
/// the runtime manages to move the page to its latest build.
@MainActor
struct SPAPresenterRefreshTests {
    @Test func refreshReloadsTheViewAfterTheLatestBuildIsUsed() async throws {
        let runtime = try MockSPARuntime(startURL: #require(URL(string: "https://test.dot")))
        let screen = try makeScreen(runtime: runtime)

        screen.presenter.didTapRefresh()
        await waitUntil { screen.view.reloadCount > 0 }

        #expect(runtime.latestBuildRequestCount == 1)
        #expect(screen.view.reloadCount == 1)
    }

    @Test func refreshStillReloadsTheViewWhenTheLatestBuildFails() async throws {
        let runtime = try MockSPARuntime(
            startURL: #require(URL(string: "https://test.dot")),
            latestBuildError: URLError(.timedOut)
        )
        let screen = try makeScreen(runtime: runtime)

        screen.presenter.didTapRefresh()
        await waitUntil { screen.view.reloadCount > 0 }

        #expect(runtime.latestBuildRequestCount == 1)
        #expect(screen.view.reloadCount == 1)
    }
}

private struct Screen {
    let presenter: SPAPresenter
    let view: StubSPAView
}

@MainActor
private func makeScreen(runtime: MockSPARuntime) throws -> Screen {
    let factory = ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))
    let host = try #require(factory.host(rawString: "test.dot"))
    let configuration = SPAConfiguration(
        title: nil,
        isRootScreen: false,
        showMoreButton: false,
        page: ProductPage(host: host)
    )
    let interactor = SPARustRuntimeInteractor(
        runtimeFactory: MockSPARuntimeFactory(runtime: runtime),
        productResolver: StubProductResolver(),
        configuration: configuration,
        logger: Logger.shared,
        productRepository: AnyDataProviderRepository(InMemoryDataProviderRepository<Product>()),
        chatProviderFactory: MockChatContactDataProviderFactory(),
        applicationStateStreams: ApplicationStateStreamFactory()
    )
    let presenter = SPAPresenter(
        interactor: interactor,
        wireframe: StubSPAWireframe(),
        configuration: configuration,
        hostProvider: StubProductHostProvider(syncHostResult: nil, asyncHostResult: .success(nil))
    )
    let view = StubSPAView()
    presenter.view = view
    interactor.presenter = presenter
    presenter.setup(engine: MockJSEngine())

    return Screen(presenter: presenter, view: view)
}
