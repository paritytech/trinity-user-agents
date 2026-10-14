import CryptoKit
import Foundation

/// An image or a file that a product gives to a chat: inline bytes, a preimage key, or an address.
///
/// Chat Extension v1 gives an address as a string: an `https` URL or an inline `data:` image. The
/// address case also carries the archive and gateway addresses of product widget images.
enum ChatResource: Hashable, Sendable {
    case inline(mimeType: String, data: Data)
    case preimage(key: Data)
    case address(URL)

    /// Reads a v1 resource string. Only an inline image or an `https` URL is a resource.
    init?(source: String) {
        if let inline = InlineDataURL(source) {
            self = .inline(mimeType: inline.mimeType, data: inline.data)
            return
        }

        guard let url = URL(string: source), url.scheme?.lowercased() == "https" else {
            return nil
        }

        self = .address(url)
    }
}

extension Chat.RoomImage {
    /// The store keeps no media type for a room image, and the resolver reads the type from the bytes.
    var resource: ChatResource {
        switch self {
        case let .inline(data): .inline(mimeType: "application/octet-stream", data: data)
        case let .preimage(key): .preimage(key: key)
        }
    }

    /// A short value that names the image, for view identity.
    var identifier: String {
        switch self {
        case let .inline(data): "inline:" + SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        case let .preimage(key): "preimage:" + key.map { String(format: "%02x", $0) }.joined()
        }
    }
}
