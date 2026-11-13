import Foundation
import TrUAPIHost

/// The products the app ships inside itself, run from their files with no
/// dotNS or Bulletin lookup. Their pages and their workers keep each product's
/// own id, so what they ask of TrUAPI is attributed to it as if it had been
/// published.
///
/// On disk each is one directory under the root, named after the product:
///
///     <product id>/
///       manifest.json          the root manifest, as it would be published
///       worker-manifest.json   the Worker manifest, as it would be published
///       worker/index.js        the Worker's entry module
///       app/index.html         the App's pages
///
/// The core reads the root and decides where a product runs from.
public struct BundledProducts: Sendable {
    /// The directory the products are read from; nil for a build that ships
    /// none.
    public let root: URL?
    /// Every product shipped, ordered by id.
    public let entries: [BundleEntry]

    /// A build that ships none.
    public static let none = BundledProducts(root: nil)

    /// Every product under `root`. A missing root ships none.
    public init(root: URL?) {
        self.root = root
        entries = root.map { bundledProducts(root: $0.path, reader: FileBundleReader()) } ?? []
    }

    /// Where the product `urlOrId` names runs from: shipped here on any
    /// network, then a development server when `devServers` allows it, then
    /// published on `networkSuffix`.
    public func source(of urlOrId: String, networkSuffix: String, devServers: Bool = false) -> ProductSource? {
        productSource(urlOrId: urlOrId, networkSuffix: networkSuffix, devServers: devServers, bundle: entries)
    }

    /// Where product `id` runs from, read on its own network.
    public func source(of id: ProductId) -> ProductSource? {
        source(of: id.rawValue, networkSuffix: id.ownNetworkSuffix)
    }
}

public extension BundleEntry {
    var directory: URL {
        URL(fileURLWithPath: path, isDirectory: true)
    }

    /// The App's pages, served the way a published App's archive is.
    var appDirectory: URL {
        directory.appendingPathComponent("app", isDirectory: true)
    }

    /// The App's page at `navigationDestination`, its id followed by a route.
    /// The id is the same on every network, so it is read against its own
    /// network rather than the app's.
    func page(navigationDestination: String) -> ProductPage? {
        ProductPage.fromNavigationDestination(navigationDestination, tld: productId.ownNetworkSuffix)
    }
}

public extension ProductWorkerSource {
    /// The Worker `entry` ships, served from its own files under its own id.
    static func bundled(_ entry: BundleEntry) -> ProductWorkerSource {
        ProductWorkerSource(contentId: entry.productId, entryRelativePath: "worker/index.js")
    }
}

/// Files on disk as the core reads them, by absolute path.
public final class FileBundleReader: BundleReader {
    public init() {}

    public func list(dir: String) -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
    }

    /// A regular file only, as the core expects: a directory is served by
    /// its `index.html`, and an archive root that is a directory is not a
    /// single-file archive.
    public func exists(path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) && !isDirectory.boolValue
    }

    public func readText(path: String) -> String? {
        FileManager.default.contents(atPath: path).flatMap { String(data: $0, encoding: .utf8) }
    }
}

private extension ProductId {
    var ownNetworkSuffix: String {
        rawValue.split(separator: ".").last.map(String.init) ?? rawValue
    }
}
