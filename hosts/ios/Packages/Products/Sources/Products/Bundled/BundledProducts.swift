import Foundation

/// A product the app ships inside itself, run from its files with no dotNS or
/// Bulletin lookup. Its pages and its worker keep the product's own id, so
/// what they ask of TrUAPI is attributed to it as if it had been published.
///
/// On disk it is one directory, named after the product:
///
///     <product id>/
///       worker-manifest.json   the Worker manifest, as it would be published
///       worker/index.js        the Worker's entry module
///       app/index.html         the App's pages
public struct BundledProduct: Hashable, Sendable {
    public let id: ProductId
    public let directory: URL

    public init(id: ProductId, directory: URL) {
        self.id = id
        self.directory = directory
    }

    /// The Worker's entry module, relative to ``directory``.
    public var workerEntryRelativePath: String {
        "worker/index.js"
    }

    /// The App's pages, served the way a published App's archive is.
    public var appDirectory: URL {
        directory.appendingPathComponent("app", isDirectory: true)
    }

    /// The Worker manifest it ships, or nil when it ships none.
    public func workerManifest(fileManager: FileManager = .default) -> String? {
        let url = directory.appendingPathComponent("worker-manifest.json")
        return fileManager.contents(atPath: url.path).flatMap { String(data: $0, encoding: .utf8) }
    }

    /// Whether it ships a Worker.
    public func hasWorker(fileManager: FileManager = .default) -> Bool {
        fileManager.fileExists(atPath: directory.appendingPathComponent(workerEntryRelativePath).path)
    }

    /// Whether it ships an App.
    public func hasApp(fileManager: FileManager = .default) -> Bool {
        fileManager.fileExists(atPath: appDirectory.appendingPathComponent(ProductBundle.indexHTML).path)
    }

    /// The App's page at `navigationDestination`, its id followed by a route.
    /// The id is the same on every network, so it is read against its own
    /// root rather than the network's top-level domain.
    public func page(navigationDestination: String) -> ProductPage? {
        guard let root = id.components(separatedBy: ProductHost.separator).last else { return nil }

        return ProductPage.fromNavigationDestination(navigationDestination, tld: root)
    }
}

/// The products shipped under one root directory, one subdirectory each.
public struct BundledProducts: Sendable {
    private let products: [ProductId: BundledProduct]

    /// A build that ships none.
    public static let none = BundledProducts(products: [:])

    /// Every product under `root`. A missing root ships none.
    public init(root: URL?, fileManager: FileManager = .default) {
        guard let root,
              let names = try? fileManager.contentsOfDirectory(atPath: root.path)
        else {
            self.init(products: [:])
            return
        }

        var products: [ProductId: BundledProduct] = [:]
        for name in names where !name.hasPrefix(".") {
            let directory = root.appendingPathComponent(name, isDirectory: true)
            var isDirectory: ObjCBool = false
            guard fileManager.fileExists(atPath: directory.path, isDirectory: &isDirectory),
                  isDirectory.boolValue
            else { continue }

            products[name] = BundledProduct(id: name, directory: directory)
        }
        self.init(products: products)
    }

    private init(products: [ProductId: BundledProduct]) {
        self.products = products
    }

    public func product(_ id: ProductId) -> BundledProduct? {
        products[id]
    }

    /// Every product shipped, ordered by id.
    public var all: [BundledProduct] {
        products.values.sorted { $0.id < $1.id }
    }
}

public extension ProductWorkerSource {
    /// The Worker `product` ships, served from its own files under its own id.
    static func bundled(_ product: BundledProduct) -> ProductWorkerSource {
        ProductWorkerSource(contentId: product.id, entryRelativePath: product.workerEntryRelativePath)
    }
}
