import CryptoKit
import Foundation
import KeyDerivation
import Products

/// The person-to-person chat product, shipped with the app (`apps/chat-product` in the
/// verbosity repository). It is installed where a hand-installed worker lives, so the
/// resolver serves it for `chat.<tld>` until a verified dotNS release replaces it.
enum BundledChatProduct {
    enum InstallError: Error {
        case missingResource(String)
        case digestMismatch(expected: String, actual: String)
    }

    /// Installs the bundled worker and its manifest for `chat.<tld>` when the installed copy differs.
    static func install(tld: String, storage: FileChatScriptStorage = FileChatScriptStorage()) throws {
        let worker = try resource("person-chat-worker", extension: "js")
        let manifest = try resource("person-chat-manifest", extension: "json")
        let digest = SHA256.hash(data: worker).map { String(format: "%02x", $0) }.joined()
        guard digest == bundledWorkerSHA256 else {
            throw InstallError.digestMismatch(expected: bundledWorkerSHA256, actual: digest)
        }

        let productId = BuiltInProduct.chat(for: tld)
        let entry = storage.chatEntrypointRelativePath()
        if storage.loadData(productId: productId, relativePath: entry) != worker {
            try storage.saveData(productId: productId, relativePath: entry, data: worker)
        }
        let manifestPath = storage.workerManifestRelativePath()
        if storage.loadData(productId: productId, relativePath: manifestPath) != manifest {
            try storage.saveData(productId: productId, relativePath: manifestPath, data: manifest)
        }
    }

    /// The manifest of the bundled worker, which declares its version.
    static func manifest() throws -> Data {
        try resource("person-chat-manifest", extension: "json")
    }

    private static func resource(_ name: String, extension ext: String) throws -> Data {
        guard let url = Bundle.main.url(forResource: name, withExtension: ext),
              let data = try? Data(contentsOf: url)
        else {
            throw InstallError.missingResource("\(name).\(ext)")
        }
        return data
    }
}
