import Foundation

public struct ProductHost: Sendable {
    static let liDomain = "li"
    static let shareRootDomains: Set<String> = ["dot", "paseo", "test"]
    static let separator = "."
    public static let shareHosts: Set<String> = Set(shareRootDomains.map { $0 + separator + liDomain })

    public let name: String
    let root: String

    /// Direct construction. Fails on an empty name or root, or a root containing a separator,
    /// or an empty sub-label inside name (e.g. "a..b").
    init?(name: String, root: String) {
        guard !name.isEmpty, !root.isEmpty, !root.contains(ProductHost.separator) else {
            return nil
        }

        let nameComponents = name.components(separatedBy: ProductHost.separator)
        guard !nameComponents.contains(where: \.isEmpty) else {
            return nil
        }

        self.name = name
        self.root = root
    }

    public func toDotDomain() -> String {
        name + ProductHost.separator + root
    }
}

extension ProductHost {
    /// Inverse of ``toDotDomain()`` when the root is unknown: productIds are
    /// always `name + separator + root` with a single root label.
    public static func name(fromDotDomain dotDomain: String) -> String? {
        let components = dotDomain.components(separatedBy: ProductHost.separator)

        guard components.count >= 2, !components.contains(where: \.isEmpty) else {
            return nil
        }

        return components.dropLast().joined(separator: ProductHost.separator)
    }

    static func parse(_ rawString: String, tld: String) -> ProductHost? {
        let parsedComponent = rawString.components(separatedBy: ProductHost.separator)

        guard !parsedComponent.contains(where: \.isEmpty) else {
            return nil
        }

        // dot-domain form: count >= 2 && last == tld
        if parsedComponent.count >= 2, parsedComponent.last == tld {
            let nameComponents = parsedComponent.dropLast()
            let name = nameComponents.joined(separator: ProductHost.separator)
            return ProductHost(name: name, root: tld)
        }

        // share form: count >= 3 && components[count-2] in shareRootDomains && last == "li"
        if parsedComponent.count >= 3, shareRootDomains.contains(parsedComponent[parsedComponent.count - 2]),
           parsedComponent.last == liDomain {
            let root = parsedComponent[parsedComponent.count - 2]
            let nameComponents = parsedComponent.dropLast(2)
            let name = nameComponents.joined(separator: ProductHost.separator)
            return ProductHost(name: name, root: root)
        }

        return nil
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
