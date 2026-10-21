import Foundation
import Keystore_iOS
import KeyDerivation
import Products
import SubstrateSdk
import Testing
@testable import polkadot_app

struct ChatProductReleasesTests {
    private final class Contract: DotNsContractApiProtocol, @unchecked Sendable {
        var record: String?
        var contentHash = Data()

        func resolveContentHash(dotNsName _: String) async throws -> Data {
            contentHash
        }

        func getMetadata(dotNsName _: String, key _: String) async throws -> String? {
            record
        }

        func clearCache() {}
    }

    private final class Fetcher: CarFetcherProtocol, @unchecked Sendable {
        var archive = Data()

        func fetchCarToFile(contentHash _: Data, onProgress _: @escaping (Int64, Int64?) -> Void) async throws -> URL {
            let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".car")
            try archive.write(to: url)
            return url
        }
    }

    // A directory holding `index.js` with `worker()`, every CID sha2-256; and the same archive with the file's
    // bytes changed under its CID.
    private static let root = "017012201e6da9ef7cda250ffe97e503360fd9188ab2006c25d2b6d5a0a73b2f774f6a30"
    private static let archive = "38a26776657273696f6e0165726f6f747381582500017012201e6da9ef7cda250ffe97e503360fd918"
        + "8ab2006c25d2b6d5a0a73b2f774f6a305a017012201e6da9ef7cda250ffe97e503360fd9188ab2006c25d2b6d5a0a73b2f774f6a30"
        + "12300a24017012207877a34b0aecd7d55bd6c33549a20445a09ebabc5058b72646b5d84e3e8eede81208696e6465782e6a730a0208"
        + "0134017012207877a34b0aecd7d55bd6c33549a20445a09ebabc5058b72646b5d84e3e8eede80a0e08021208776f726b6572282918"
        + "08"
    private static let tampered = "38a26776657273696f6e0165726f6f747381582500017012201e6da9ef7cda250ffe97e503360fd9"
        + "188ab2006c25d2b6d5a0a73b2f774f6a305a017012201e6da9ef7cda250ffe97e503360fd9188ab2006c25d2b6d5a0a73b2f774f6a"
        + "3012300a24017012207877a34b0aecd7d55bd6c33549a20445a09ebabc5058b72646b5d84e3e8eede81208696e6465782e6a730a02"
        + "080134017012207877a34b0aecd7d55bd6c33549a20445a09ebabc5058b72646b5d84e3e8eede80a0e080212086576696c28292121"
        + "1808"

    private let contract = Contract()
    private let fetcher = Fetcher()
    private let settings = InMemorySettingsManager()
    private let storage = FileChatScriptStorage()
    // A network of its own for each test, so no test sees the files of another.
    private let tld = "test\(UInt32.random(in: 0 ... .max))"

    private func releases() -> ChatProductReleases {
        ChatProductReleases(
            contractApi: contract,
            carFetcher: fetcher,
            storage: storage,
            settings: settings,
            bundledManifest: { Data(Self.record(version: "[0,1,0]").utf8) }
        )
    }

    private static func record(version: String) -> String {
        "{\"$v\":1,\"kind\":\"worker\",\"appVersion\":\(version),\"entrypoint\":\"index.js\","
            + "\"includes\":{\"chat\":true,\"pocket\":false},\"keepAlive\":\"foreground\"}"
    }

    private func installedEntry() -> String? {
        storage.loadData(productId: BuiltInProduct.chat(for: tld), relativePath: storage.chatEntrypointRelativePath())
            .map { String(decoding: $0, as: UTF8.self) }
    }

    private func cleanUp() {
        let directory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Products")
        try? FileManager.default.removeItem(at: directory.appendingPathComponent(BuiltInProduct.chat(for: tld)))
    }

    @Test func aNewerReleaseWhoseArchiveIsItsContenthashIsInstalled() async throws {
        defer { cleanUp() }
        contract.record = Self.record(version: "[0,2,0]")
        contract.contentHash = try Data(hexString: Self.root)
        fetcher.archive = try Data(hexString: Self.archive)

        await releases().update(tld: tld)

        #expect(installedEntry() == "worker()")
        // An app start with the older bundle keeps the release.
        try releases().installBundled(tld: tld)
        #expect(installedEntry() == "worker()")
    }

    @Test func aTamperedArchiveOrAnOlderReleaseIsNotInstalled() async throws {
        defer { cleanUp() }
        contract.contentHash = try Data(hexString: Self.root)
        contract.record = Self.record(version: "[0,2,0]")
        fetcher.archive = try Data(hexString: Self.tampered)
        await releases().update(tld: tld)
        #expect(installedEntry() == nil)

        contract.record = Self.record(version: "[0,1,0]")
        fetcher.archive = try Data(hexString: Self.archive)
        await releases().update(tld: tld)
        #expect(installedEntry() == nil)
    }
}
