import Testing
@testable import Products

/// One rule for where a worker is served from, so chat and the Pocket cannot
/// drift apart about which product has one.
struct ProductWorkerSourceTests {
    @Test func servesAPublishedWorkerThatIncludesTheModality() throws {
        let resolved = product(worker: worker(modalities: [.chat, .pocket([])]))

        #expect(ProductWorkerSource.published(for: resolved, serving: .chat)?.contentId == "worker.game.dot")
        #expect(ProductWorkerSource.published(for: resolved, serving: .pocket)?.contentId == "worker.game.dot")
    }

    /// A worker that does not declare the modality serves none of it. Falling
    /// back to a hand-installed script here would hand a product's cards to a
    /// file the product never published.
    @Test func servesNothingWhenThePublishedWorkerOmitsTheModality() throws {
        let resolved = product(worker: worker(modalities: [.chat]))

        #expect(ProductWorkerSource.published(for: resolved, serving: .pocket) == nil)
        #expect(ProductWorkerSource.published(for: resolved, serving: .chat) != nil)
    }

    /// The fallback is for a product that published no worker at all, which is
    /// what a script installed through debug settings stands in for.
    @Test func fallsBackOnlyWhenNoWorkerIsPublished() throws {
        let resolved = product(worker: nil)

        #expect(ProductWorkerSource.published(for: resolved, serving: .pocket) == nil)
        #expect(
            ProductWorkerSource.installedByHand(for: resolved, entryPath: { _ in "bot.js" })?.contentId == "game.dot"
        )
        #expect(ProductWorkerSource.installedByHand(for: resolved, entryPath: { _ in nil }) == nil)
    }

    private func worker(modalities: [ProductExecutable.Worker.Modality]) -> ProductExecutable.Worker {
        ProductExecutable.Worker(
            identifier: "worker.game.dot",
            appVersion: SemVer(major: 1, minor: 0, patch: 0, build: nil),
            entrypoint: "index.js",
            modalities: modalities
        )
    }

    private func product(worker: ProductExecutable.Worker?) -> ResolvedProduct {
        ResolvedProduct(
            id: "game.dot",
            displayName: "Game",
            description: nil,
            icon: nil,
            executables: ProductExecutables(app: nil, widget: nil, worker: worker),
            hasManifest: true
        )
    }
}
