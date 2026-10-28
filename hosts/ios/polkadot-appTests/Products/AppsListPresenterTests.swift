import PolkadotUI
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
struct AppsListPresenterTests {
    @Test func selectionOpensTheProductCarriedByTheItem() throws {
        let productId = try ProductId("game.dot")
        let wireframe = RecordingAppsWireframe()
        let presenter = AppsListPresenter(
            interactor: IdleAppsInteractor(),
            wireframe: wireframe,
            viewModelFactory: EmptyAppsViewModelFactory()
        )
        let item = AppsListViewLayout<ProductId>.Item(
            id: productId,
            name: "Game",
            subtitle: nil,
            avatar: .colored(text: "G", colorSeed: productId.rawValue),
            icon: nil
        )

        presenter.selectApp(item)

        #expect(wireframe.opened == [productId])
    }
}

@MainActor
private final class RecordingAppsWireframe: AppsListWireframeProtocol {
    var opened: [ProductId] = []

    func showAppDetail(productId: ProductId, from _: AppsListViewProtocol?) {
        opened.append(productId)
    }
}

private final class IdleAppsInteractor: AppsListInteractorInputProtocol {
    func setup() {}
}

private struct EmptyAppsViewModelFactory: AppsListViewModelMaking {
    func createItems(from _: [ResolvedProduct]) -> [AppsListViewLayout<ProductId>.Item] { [] }
}
