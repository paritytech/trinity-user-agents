import Foundation
import AsyncExtensions
import PolkadotUI
import Products
import Testing
@testable import polkadot_app

/// The sheet shows the card as it will look, so what it draws has to be what
/// the Pocket draws. A face whose images do not resolve here is approved
/// against blank space.
@MainActor
struct PocketAddCardViewModelTests {
    @Test
    func drawsTheOfferedProductsOwnImages() async throws {
        let asked = Recorder()
        let viewModel = PocketAddCardViewModel(
            productId: "game.paseo",
            cardId: PocketCardId(value: "loyalty"),
            interactor: makeAddCardInteractor(published: [loyalty]),
            images: { productId in
                asked.note(productId)
                return PocketImageResolver(
                    contentId: { productId },
                    archive: ProductWorkerArchive(
                        dotNsResolver: StubArchiveRoot(),
                        cachedRoot: { _ in nil }
                    ),
                    ipfsUrl: { _ in nil }
                )
            }
        )

        let resolved = await viewModel.resolveImage?(.archive(path: "logo.png"))

        #expect(asked.products == ["game.paseo"])
        #expect(resolved?.lastPathComponent == "logo.png")
    }

    /// A card that could not be stored must not dismiss the sheet as approved:
    /// the user would be left believing the Pocket holds a card it never took.
    @Test
    func refusesTheOfferWhenTheCardCouldNotBeStored() async throws {
        let store = InMemoryPocketCardStore()
        await store.failWrites()
        let dismissal = Dismissal()
        let viewModel = makeViewModel(store: store)
        viewModel.onFinish = { dismissal.note() }

        await viewModel.load()
        await viewModel.add()

        guard case let .refused(message) = viewModel.state else {
            Issue.record("the sheet stayed on the offer")
            return
        }
        #expect(message == String(localized: .Products.pocketAddCardFailed))
        #expect(!dismissal.happened)
    }
}

@MainActor
private func makeViewModel(store: any PocketCardStore) -> PocketAddCardViewModel {
    PocketAddCardViewModel(
        productId: "game.paseo",
        cardId: PocketCardId(value: "loyalty"),
        interactor: makeAddCardInteractor(published: [loyalty], store: store),
        images: { _ in nil }
    )
}

private let loyalty = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty",
    preview: .archive(path: "faces/loyalty.json")
)

/// Whether the sheet dismissed as approved.
@MainActor
private final class Dismissal {
    private(set) var happened = false

    func note() {
        happened = true
    }
}

@MainActor
private final class Recorder {
    private(set) var products: [ProductId] = []

    func note(_ productId: ProductId) {
        products.append(productId)
    }
}

private struct StubArchiveRoot: DotNsResolverProtocol {
    func resolveToLocalURL(dotNsName _: String) async throws -> URL {
        FileManager.default.temporaryDirectory
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
