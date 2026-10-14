import CryptoKit
import Foundation
import FoundationExt

struct W3sMerchant: Decodable, Equatable {
    /// 32-byte topic the merchant terminal is listening on.
    let topic: Data
    /// 32-byte X25519 public key.
    let key: Data
    /// Optional human-readable label shown to the payer as the recipient.
    /// Blank/whitespace-only values are normalised to `nil` so the caller's
    /// fallback (cash-register serial) kicks in for misconfigured entries.
    let name: String?

    init(topic: Data, key: Data, name: String? = nil) {
        self.topic = topic
        self.key = key
        self.name = name
    }

    private enum CodingKeys: String, CodingKey {
        case topic
        case key
        case name
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        let topicString = try container.decode(String.self, forKey: .topic)
        guard let topic = Data(base64URLEncoded: topicString), topic.count == 32 else {
            throw DecodingError.dataCorruptedError(
                forKey: .topic,
                in: container,
                debugDescription: "Expected base64url-encoded 32-byte topic"
            )
        }

        let keyString = try container.decode(String.self, forKey: .key)
        guard
            let key = Data(base64URLEncoded: keyString),
            (try? Curve25519.KeyAgreement.PublicKey(rawRepresentation: key)) != nil
        else {
            throw DecodingError.dataCorruptedError(
                forKey: .key,
                in: container,
                debugDescription: "Expected base64url-encoded 32-byte X25519 key"
            )
        }

        let rawName = try container.decodeIfPresent(String.self, forKey: .name)
        let trimmedName = rawName?.trimmingCharacters(in: .whitespacesAndNewlines)

        self.topic = topic
        self.key = key
        name = (trimmedName?.isEmpty ?? true) ? nil : trimmedName
    }
}
