import Foundation
import Products
import Testing
@testable import polkadot_app

struct AppsListInteractorTests {
    @Test
    func listsProductsGrantedInTheCoreAfterTheNativeOnes() async throws {
        let core = RecordingCoreAuthorizations()
        core.grantsToReturn = [
            grant("zeta.paseo", .balanceAccess),
            grant("alpha.paseo", .webRtcAccess),
            grant("game.paseo", .deviceCapability(.camera))
        ]
        let (interactor, presenter) = await makeInteractor(
            nativeGrants: [grant("game.paseo", .deviceCapability(.camera)), grant("legacy.paseo", .webRtcAccess)],
            core: core
        )

        interactor.setup()
        let listed = try await waitUntil { await presenter.lastProductIds.count == 4 }

        let expected = ["game.paseo", "legacy.paseo", "alpha.paseo", "zeta.paseo"]
        #expect(listed)
        #expect(await presenter.lastProductIds == expected)
    }

    @Test
    func readsTheCoreAgainWhenAnyDecisionMoves() async throws {
        let core = RecordingCoreAuthorizations()
        let (interactor, presenter) = await makeInteractor(core: core)

        interactor.setup()
        _ = try await waitUntil { core.reads == 1 }
        core.grantsToReturn = [grant("game.paseo", .balanceAccess)]
        core.changed(productId: "game.paseo")
        let relisted = try await waitUntil { await presenter.lastProductIds == ["game.paseo"] }

        #expect(relisted)
        #expect(core.reads == 2)
    }

    @Test
    func listsTheNativeProductsWhenTheCoreCannotBeRead() async throws {
        let core = RecordingCoreAuthorizations()
        core.grantsError = TestError()
        let (interactor, presenter) = await makeInteractor(
            nativeGrants: [grant("legacy.paseo", .webRtcAccess)],
            core: core
        )

        interactor.setup()
        let listed = try await waitUntil { await presenter.lastProductIds == ["legacy.paseo"] }

        #expect(listed)
    }
}

private struct TestError: Error {}

@MainActor
private func makeInteractor(
    nativeGrants: [ProductPermissionGrant] = [],
    core: RecordingCoreAuthorizations
) -> (AppsListInteractor, ProductsSpy) {
    let interactor = AppsListInteractor(
        providerFactory: SnapshotGrantsProvider(grants: nativeGrants),
        coreAuthorizations: core,
        productResolver: StubProductResolver()
    )
    let presenter = ProductsSpy()
    interactor.presenter = presenter
    return (interactor, presenter)
}

@MainActor
private final class ProductsSpy: AppsListInteractorOutputProtocol {
    private(set) var lastProductIds: [ProductId] = []

    func didReceive(products: [ResolvedProduct]) {
        lastProductIds = products.map(\.id)
    }
}
