import Foundation
import TrUAPIHost

public struct ProductHost: Sendable {
    public static let shareHosts: Set<String> = Set(productShareHosts())

    private let identifier: ProductId

    public var name: String {
        productIdName(productId: identifier)!
    }

    init?(name: String, root: String) {
        guard let identifier = try? productIdFromName(name: name, networkSuffix: root) else { return nil }
        self.identifier = identifier
    }

    init(link: ProductLink) {
        identifier = link.identifier
    }

    public func toDotDomain() -> ProductId {
        identifier
    }
}

extension ProductHost {
    public static func name(fromDotDomain dotDomain: ProductId) -> String {
        productIdName(productId: dotDomain) ?? ""
    }

    static func parse(_ rawString: String, tld: String) -> ProductHost? {
        parseProductLink(input: rawString, networkSuffix: tld).map { ProductHost(link: $0) }
    }

    static func fromUrl(_ url: URL, tld: String) -> ProductHost? {
        parse(url.absoluteString, tld: tld)
    }
}
