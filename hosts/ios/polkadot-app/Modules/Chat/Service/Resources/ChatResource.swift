import CryptoKit
import Foundation

/// An image or a file that a product gives to a chat: inline bytes, a preimage key, a sealed file, or an address.
///
/// Chat Extension v1 gives an address as a string: an `https` URL or an inline `data:` image. The
/// address case also carries the archive and gateway addresses of product widget images.
enum ChatResource: Hashable, Sendable {
    case inline(mimeType: String, data: Data)
    case preimage(key: Data)
    case sealed(ChatSealedResource)
    case address(URL)

    /// Reads a resource string: an inline image, an `https` URL, or a preimage key that a Chat Extension v2
    /// product body keeps (see ``preimageSource(key:)``).
    init?(source: String) {
        if let inline = InlineDataURL(source) {
            self = .inline(mimeType: inline.mimeType, data: inline.data)
            return
        }

        if let key = Self.preimageKey(source: source) {
            self = .preimage(key: key)
            return
        }

        if let sealed = Self.sealed(source: source) {
            self = .sealed(sealed)
            return
        }

        guard let url = URL(string: source), url.scheme?.lowercased() == "https" else {
            return nil
        }

        self = .address(url)
    }
}

extension ChatResource {
    private static let preimageScheme = "preimage:"

    /// The source string of a preimage key: `preimage:` and the key in lowercase hex.
    static func preimageSource(key: Data) -> String {
        preimageScheme + key.map { String(format: "%02x", $0) }.joined()
    }

    private static let sealedScheme = "sealed:"

    /// The source string of a sealed file: `sealed:` and its JSON in base64url. The string holds the key of the file,
    /// so it stays on this device.
    static func sealedSource(_ sealed: ChatSealedResource) -> String {
        let json = (try? JSONEncoder().encode(sealed)) ?? Data()
        return sealedScheme + json.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
    }

    private static func sealed(source: String) -> ChatSealedResource? {
        guard source.hasPrefix(sealedScheme) else { return nil }

        let base64 = source.dropFirst(sealedScheme.count)
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        return Data(base64Encoded: base64).flatMap { try? JSONDecoder().decode(ChatSealedResource.self, from: $0) }
    }

    private static func preimageKey(source: String) -> Data? {
        guard source.hasPrefix(preimageScheme) else { return nil }

        let hex = Array(source.dropFirst(preimageScheme.count).utf8)
        guard !hex.isEmpty, hex.count.isMultiple(of: 2) else { return nil }

        var key = Data(capacity: hex.count / 2)
        for index in stride(from: 0, to: hex.count, by: 2) {
            guard let high = nibble(hex[index]), let low = nibble(hex[index + 1]) else {
                return nil
            }
            key.append(high << 4 | low)
        }
        return key
    }

    private static func nibble(_ character: UInt8) -> UInt8? {
        switch character {
        case UInt8(ascii: "0") ... UInt8(ascii: "9"): character - UInt8(ascii: "0")
        case UInt8(ascii: "a") ... UInt8(ascii: "f"): character - UInt8(ascii: "a") + 10
        case UInt8(ascii: "A") ... UInt8(ascii: "F"): character - UInt8(ascii: "A") + 10
        default: nil
        }
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
