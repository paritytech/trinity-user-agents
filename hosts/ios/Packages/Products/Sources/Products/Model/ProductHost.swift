import struct TrUAPIHost.ProductId
import Foundation

public struct ProductHost: Sendable {
    static let liDomain = "li"
    static let shareRootDomains: Set<String> = ["dot", "paseo", "testnet"]
    static let separator = "."
    public static let shareHosts: Set<String> = Set(shareRootDomains.map { $0 + separator + liDomain })

    private let identifier: ProductId

    public var name: String {
        identifier.rawValue.split(separator: ".").dropLast().joined(separator: ".")
    }

    var root: String {
        String(identifier.rawValue.split(separator: ".").last!)
    }

    init?(name: String, root: String) {
        guard !root.contains(ProductHost.separator),
              let identifier = try? ProductId("\(name).\(root)")
        else { return nil }

        self.identifier = identifier
    }

    private init(identifier: ProductId) {
        self.identifier = identifier
    }

    public func toDotDomain() -> ProductId {
        identifier
    }
}

extension ProductHost {
    public static func name(fromDotDomain dotDomain: ProductId) -> String {
        dotDomain.rawValue.split(separator: ".").dropLast().joined(separator: ".")
    }

    static func parse(_ rawString: String, tld: String) -> ProductHost? {
        if let identifier = try? ProductId(rawString),
           identifier.rawValue.hasSuffix(".\(tld)") {
            return ProductHost(identifier: identifier)
        }

        let shareSuffix = separator + liDomain
        guard rawString.hasSuffix(shareSuffix),
              let identifier = try? ProductId(String(rawString.dropLast(shareSuffix.count)))
        else { return nil }

        let host = ProductHost(identifier: identifier)
        return shareRootDomains.contains(host.root) ? host : nil
    }

    static func fromUrl(_ url: URL, tld: String) -> ProductHost? {
        guard let rawHost = url.host() else {
            return nil
        }

        return parse(rawHost, tld: tld)
    }

    static func fromNavigationDestination(_ dest: String, tld: String) -> ProductHost? {
        guard let url = NavigationDestinationURL.make(dest), url.host() != nil else {
            return parse(dest, tld: tld)
        }

        return ProductHost.fromUrl(url, tld: tld)
    }
}
