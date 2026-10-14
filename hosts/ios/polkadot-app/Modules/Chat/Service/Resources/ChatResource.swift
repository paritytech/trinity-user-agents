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
