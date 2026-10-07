import Foundation
import Products
import Testing
@testable import polkadot_app

/// The pieces a card's face is loaded and approved through, shared so the
/// preview loader, the interactor and the approval sheet are all exercised
/// against the same published card.

let minimalFace = Data("""
{
  "tag": "Column",
  "value": {
    "modifiers": [],
    "props": {},
    "children": [
      {
        "tag": "Text",
        "value": { "modifiers": [], "props": {}, "children": [{ "tag": "String", "value": { "text": "Loyalty" } }] }
      }
    ]
  }
}
""".utf8)

let refusingFetch: @Sendable (URL, Int) async throws -> Data = { _, _ in
    Issue.record("the archive path must not reach the network")
    return Data()
}

func makeAddCardInteractor(
    published: [PocketCardDefinition],
    includesPocket: Bool = true,
    store: any PocketCardStore = InMemoryPocketCardStore()
) -> PocketAddCardInteractor {
    PocketAddCardInteractor(
        publishedCards: StubCardCatalog(
            definitions: published,
            includesPocket: includesPocket,
            productName: "Game"
        ),
        previews: PocketPreviewLoader(archive: StubArchive(contents: minimalFace), fetch: refusingFetch),
        store: store
    )
}

struct StubCardCatalog: PublishedPocketCardsResolving {
    let definitions: [PocketCardDefinition]
    let includesPocket: Bool
    let productName: String

    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard {
        guard includesPocket else { throw PocketPublishError.noPocket }
        guard let definition = definitions.first(where: { $0.id == cardId }) else {
            throw PocketPublishError.unknownCard
        }
        return PublishedPocketCard(
            productId: productId,
            productName: productName,
            workerContentId: "worker.\(productId)",
            definition: definition
        )
    }
}

final class StubArchive: PocketArchiveReading, @unchecked Sendable {
    let contents: Data?
    /// The bound the loader passed down, so a loader that stopped telling the
    /// reader how much to read is caught here rather than at the bound itself.
    private(set) var boundedTo: Int?

    init(contents: Data?) {
        self.contents = contents
    }

    func file(contentId _: ProductId, path: String, maxBytes: Int) async throws -> Data {
        boundedTo = maxBytes
        guard let contents else { throw PocketPreviewError.notReachable(path) }
        return contents
    }
}
