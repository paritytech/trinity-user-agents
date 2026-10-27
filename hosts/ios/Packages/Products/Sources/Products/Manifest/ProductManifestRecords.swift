import struct TrUAPIHost.ProductId
import Foundation

/// dotNS conventions the manifest specification fixes: text-record keys and the
/// `<kind>.<base>` subname layout.
public enum ProductManifestRecords {
    public static let rootKey = "manifest"
    public static let executableKey = "executable"

    /// Prepends the kind label to a whole base name, whatever TLD it carries:
    /// `subname(base: "coinflip.<tld>", kind: .app) == "app.coinflip.<tld>"`
    public static func subname(base: ProductId, kind: ExecutableKind) throws -> ProductId {
        try ProductId("\(kind.rawValue).\(base)")
    }

    /// Inverse of ``subname(base:kind:)``. The prefix comes off only when the remainder still holds
    /// a dot, otherwise `app.<tld>` — a product named after a kind — would collapse to the bare TLD.
    public static func baseName(of host: ProductId) throws -> ProductId {
        for kind in ExecutableKind.allCases {
            let prefix = "\(kind.rawValue)."

            guard host.rawValue.hasPrefix(prefix) else { continue }

            let remainder = String(host.rawValue.dropFirst(prefix.count))
            return remainder.contains(".") ? try ProductId(remainder) : host
        }

        return host
    }
}
