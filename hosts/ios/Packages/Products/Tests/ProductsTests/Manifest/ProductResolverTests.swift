import struct TrUAPIHost.ProductId
import Foundation
import Testing
@testable import Products

struct ProductResolverTests {
    @Test func fallsBackToALegacyAppWhenNoRootManifestIsPublished() async throws {
        let resolved = try await makeResolver(StubDotNsResolver()).resolve(ProductId("hackm3.dot"))

        #expect(!resolved.hasManifest)
        #expect(resolved.displayName == "hackm3.dot")
        #expect(resolved.icon == nil)
        #expect(resolved.executables.app?.identifier.rawValue == "hackm3.dot")
        #expect(resolved.executables.worker == nil)
        #expect(resolved.appContentId.rawValue == "hackm3.dot")
    }

    @Test func propagatesUnavailableNetworkBeforeReadingRecords() async throws {
        let stub = StubDotNsResolver()
        let resolver = ProductResolver(
            dotNsResolver: stub,
            hostProvider: ProductHostFactory(tldProvider: FailingTldProvider()),
            logger: SilentLogger(),
            screening: .passingThrough
        )

        await #expect(throws: FailingTldProvider.Unreachable.self) {
            try await resolver.resolve(ProductId("worker.hackm3.dot"))
        }
        #expect(stub.readCount == 0)
    }

    @Test(arguments: ["hackm3.paseo", "worker.hackm3.paseo", "localhost:3000"])
    func rejectsProductsOutsideTheNetworkBeforeReadingRecords(_ input: String) async throws {
        let stub = StubDotNsResolver()
        let resolver = makeResolver(stub)

        await #expect(throws: ProductResolutionError.self) {
            try await resolver.resolve(ProductId(input))
        }
        #expect(stub.readCount == 0)
    }

    @Test func readsMetadataAndEveryPublishedExecutable() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.worker, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.hasManifest)
        #expect(resolved.displayName == "HackM3")
        #expect(resolved.description == "A hackathon product")
        #expect(resolved.icon?.cid == "bafyicon")
        #expect(resolved.appContentId.rawValue == "app.hackm3.dot")
        #expect(resolved.executables.worker?.entrypoint == "src/worker.js")
    }

    /// An expanded Pocket card runs the widget, so the subname is read: a
    /// product whose widget went unread opens its app instead, which is a
    /// different archive and a different screen.
    @Test func readsTheWidgetAnExpandedCardRuns() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.widget, name: "widget.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.widget?.identifier.rawValue == "widget.hackm3.dot")
        #expect(resolved.contentId(for: .widget).rawValue == "widget.hackm3.dot")
    }

    /// A product that publishes no executable of a kind is served from its base
    /// name, which is where a legacy product and a script installed by hand
    /// both live. Every caller asking which archive backs a surface has to get
    /// the same answer.
    @Test func servesAnUnpublishedKindFromTheBaseName() async throws {
        let resolved = try await makeResolver(StubDotNsResolver()).resolve(ProductId("hackm3.dot"))

        #expect(resolved.contentId(for: .app).rawValue == "hackm3.dot")
        #expect(resolved.contentId(for: .widget).rawValue == "hackm3.dot")
        #expect(resolved.contentId(for: .worker).rawValue == "hackm3.dot")
    }

    @Test func servesEachPublishedKindFromItsOwnSubname() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.widget, name: "widget.hackm3.dot", key: "executable")
        stub.set(Fixtures.worker, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.contentId(for: .app).rawValue == "app.hackm3.dot")
        #expect(resolved.contentId(for: .widget).rawValue == "widget.hackm3.dot")
        #expect(resolved.contentId(for: .worker).rawValue == "worker.hackm3.dot")
    }

    @Test func skipsAMalformedExecutableWithoutLosingItsSiblings() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set("{ not json", name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.app != nil)
        #expect(resolved.executables.worker == nil)
    }

    /// Chat and Pocket ride on one worker record, so a pocket section the
    /// publisher typed wrong must not take the product's chat off the air.
    @Test func keepsAChatWorkerWhoseProductPublishedAMalformedPocketSection() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.workerWithMalformedPocket, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.worker?.serves(.chat) == true)
        #expect(resolved.executables.worker?.entrypoint == "src/worker.js")
        #expect(resolved.executables.worker?.pocketCards.isEmpty == true)
    }

    @Test func failsWhenTheRootManifestItselfIsMalformed() async {
        let stub = StubDotNsResolver()
        stub.set(#"{"$v":9,"displayName":"x"}"#, name: "hackm3.dot", key: "manifest")

        await #expect(throws: ProductResolutionError.self) {
            try await makeResolver(stub).resolve(ProductId("hackm3.dot"))
        }
    }

    @Test func acceptsASubnameAndResolvesItsBase() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolved = try await makeResolver(stub).resolve(ProductId("worker.hackm3.dot"))

        #expect(resolved.id.rawValue == "hackm3.dot")
        #expect(resolved.displayName == "HackM3")
    }

    @Test func servesRepeatResolutionsFromCache() async throws {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolver = makeResolver(stub)
        _ = try await resolver.resolve(ProductId("hackm3.dot"))
        let readsAfterFirst = stub.readCount

        _ = try await resolver.resolve(ProductId("hackm3.dot"))

        #expect(stub.readCount == readsAfterFirst)
    }

    /// Caching a partial read would serve the base name's archive for the rest of the session.
    @Test func propagatesAFailedExecutableReadInsteadOfReportingNoExecutable() async {
        let stub = StubDotNsResolver()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.failingNames = ["app.hackm3.dot"]

        await #expect(throws: (any Error).self) {
            try await makeResolver(stub).resolve(ProductId("hackm3.dot"))
        }
    }

    /// A read failure must stay retryable, so it is never cached.
    @Test func doesNotCacheAFailedResolution() async throws {
        let stub = StubDotNsResolver()
        stub.readError = URLError(.notConnectedToInternet)

        let resolver = makeResolver(stub)
        _ = try? await resolver.resolve(ProductId("hackm3.dot"))

        stub.readError = nil
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolved = try await resolver.resolve(ProductId("hackm3.dot"))

        #expect(resolved.displayName == "HackM3")
    }
}

private func makeResolver(_ stub: StubDotNsResolver) -> ProductResolver {
    ProductResolver(
        dotNsResolver: stub,
        hostProvider: ProductHostFactory(tldProvider: StubTldProvider()),
        logger: SilentLogger(),
        screening: .passingThrough
    )
}

private enum Fixtures {
    static let root = """
    {"$v":1,"displayName":"HackM3","description":"A hackathon product",
     "icon":{"cid":"bafyicon","format":"png"}}
    """

    static let app = #"{"$v":1,"kind":"app","appVersion":[1,0,0]}"#

    static let widget = """
    {"$v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[2],"width":4}}
    """

    static let worker = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"src/worker.js",
     "includes":{"chat":true,"pocket":false}}
    """

    static let workerWithMalformedPocket = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"src/worker.js",
     "includes":{"chat":true,"pocket":true},"pocket":{"cards":{"loyalty":"Loyalty"}}}
    """
}
