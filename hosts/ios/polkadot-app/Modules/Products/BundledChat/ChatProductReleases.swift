import CarParser
import CryptoKit
import Foundation
import Keystore_iOS
import KeyDerivation
import Products
import SubstrateSdk

/// Which release of the chat product runs (HX4): the bundled one, or a newer one from dotNS that passed every check.
/// Its archive is the content of the worker's contenthash with every block matching its CID, its version is above
/// the bundled one and the one installed, and it has the worker's entry file. A release that passes is installed
/// where the bundle goes and runs from the next start of the worker; an app release with a newer bundle replaces it.
final class ChatProductReleases: @unchecked Sendable {
    struct Installed: Codable, Equatable {
        let version: [Int]
        let contentHash: String
    }

    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let storage: FileChatScriptStorage
    private let settings: SettingsManagerProtocol
    private let bundledManifest: () throws -> Data
    private let logger: LoggerProtocol

    init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: FileChatScriptStorage = FileChatScriptStorage(),
        settings: SettingsManagerProtocol = SettingsManager.shared,
        bundledManifest: @escaping () throws -> Data = { try BundledChatProduct.manifest() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.storage = storage
        self.settings = settings
        self.bundledManifest = bundledManifest
        self.logger = logger
    }

    /// Installs the bundled worker, unless a newer release that passed its checks is installed.
    func installBundled(tld: String) throws {
        if let installed, try bundledVersion().lexicographicallyPrecedes(installed.version) { return }
        try BundledChatProduct.install(tld: tld, storage: storage)
        settings.removeValue(for: Self.installedKey)
    }

    /// Looks for a release newer than the one that runs and installs it once it passes.
    func update(tld: String) async {
        do {
            try await updateThrowing(tld: tld)
        } catch {
            logger.warning("The chat product's dotNS release was not taken: \(error)")
        }
    }

    static func digest(code: Int, block: Data) throws -> Data? {
        switch code {
        case 0x12: Data(SHA256.hash(data: block))
        case 0xB220: try block.blake2b32()
        case 0x00: block
        default: nil
        }
    }
}

private extension ChatProductReleases {
    static let installedKey = "chatProductRelease"

    enum ReleaseError: Error {
        case noEntry(String)
    }

    struct WorkerRecord: Decodable {
        let appVersion: [Int]
        let entrypoint: String
    }

    var installed: Installed? {
        settings.data(for: Self.installedKey).flatMap { try? JSONDecoder().decode(Installed.self, from: $0) }
    }

    func bundledVersion() throws -> [Int] {
        try JSONDecoder().decode(WorkerRecord.self, from: bundledManifest()).appVersion
    }

    func updateThrowing(tld: String) async throws {
        let productId = BuiltInProduct.chat(for: tld)
        let host = ProductManifestRecords.subname(base: productId, kind: .worker)
        guard let text = try await contractApi.getMetadata(dotNsName: host, key: ProductManifestRecords.executableKey),
              let record = try? JSONDecoder().decode(WorkerRecord.self, from: Data(text.utf8))
        else { return }
        let floor = [try bundledVersion(), installed?.version ?? []].max { $0.lexicographicallyPrecedes($1) } ?? []
        guard floor.lexicographicallyPrecedes(record.appVersion) else { return }

        let contentHash = try await contractApi.resolveContentHash(dotNsName: host)
        let car = try await carFetcher.fetchCarToFile(contentHash: contentHash, onProgress: { _, _ in })
        defer { try? FileManager.default.removeItem(at: car) }
        try CarParser.verify(fileURL: car, root: contentHash, digest: Self.digest)

        var files: [String: Data] = [:]
        try CarParser.unpack(fileURL: car) { path, produce in
            var bytes = Data()
            try produce { bytes.append($0) }
            files[path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))] = bytes
        }
        let entryPath = record.entrypoint.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let entry = files[entryPath] else { throw ReleaseError.noEntry(entryPath) }

        try storage.saveData(productId: productId, relativePath: storage.chatEntrypointRelativePath(), data: entry)
        try storage.saveData(productId: productId, relativePath: storage.workerManifestRelativePath(), data: Data(text.utf8))
        let release = Installed(version: record.appVersion, contentHash: contentHash.toHex())
        settings.set(value: try JSONEncoder().encode(release), for: Self.installedKey)
        logger.info("The chat product's release \(record.appVersion) passed its checks and runs from the next start")
    }
}
