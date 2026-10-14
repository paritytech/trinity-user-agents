import CryptoKit
import Foundation
import Testing

@testable import polkadot_app

@Suite("W3sMerchant")
struct W3sMerchantTests {
    private let decoder = JSONDecoder()

    /// A 32-byte X25519 public key derived from a fresh keypair.
    private static func validX25519Key() -> Data {
        Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
    }

    @Test("Decodes a well-formed merchant entry")
    func decodesValid() throws {
        let topicBytes = Data(repeating: 0xAA, count: 32)
        let keyBytes = Self.validX25519Key()
        let json = """
        {
          "topic": "\(topicBytes.base64URLEncodedString())",
          "key": "\(keyBytes.base64URLEncodedString())"
        }
        """
        let merchant = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        #expect(merchant.topic == topicBytes)
        #expect(merchant.key == keyBytes)
        #expect(merchant.name == nil)
    }

    @Test("Decodes the optional name field when present")
    func decodesName() throws {
        let topicBytes = Data(repeating: 0xAA, count: 32)
        let keyBytes = Self.validX25519Key()
        let json = """
        {
          "topic": "\(topicBytes.base64URLEncodedString())",
          "key": "\(keyBytes.base64URLEncodedString())",
          "name": "Café Müller"
        }
        """
        let merchant = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        #expect(merchant.name == "Café Müller")
    }

    @Test("Treats blank / whitespace-only name as missing")
    func treatsBlankNameAsNil() throws {
        let topicBytes = Data(repeating: 0xAA, count: 32)
        let keyBytes = Self.validX25519Key()
        // JSON-escaped sequences so the values arrive at the decoder as literal
        // whitespace inside the string, not as malformed control bytes in JSON.
        let blanks = ["", "   ", #"\t\n  "#]
        for blank in blanks {
            let json = """
            {
              "topic": "\(topicBytes.base64URLEncodedString())",
              "key": "\(keyBytes.base64URLEncodedString())",
              "name": "\(blank)"
            }
            """
            let merchant = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
            #expect(merchant.name == nil, "expected nil for \"\(blank)\"")
        }
    }

    @Test("Trims surrounding whitespace from a non-blank name")
    func trimsSurroundingWhitespace() throws {
        let topicBytes = Data(repeating: 0xAA, count: 32)
        let keyBytes = Self.validX25519Key()
        let json = """
        {
          "topic": "\(topicBytes.base64URLEncodedString())",
          "key": "\(keyBytes.base64URLEncodedString())",
          "name": "  Café Müller  "
        }
        """
        let merchant = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        #expect(merchant.name == "Café Müller")
    }

    @Test("Decodes a map of merchants keyed by serial")
    func decodesMap() throws {
        let topicBytes = Data(repeating: 0xCC, count: 32)
        let keyBytes = Self.validX25519Key()
        let json = """
        {
          "REG-001": {
            "topic": "\(topicBytes.base64URLEncodedString())",
            "key": "\(keyBytes.base64URLEncodedString())"
          }
        }
        """
        let map = try decoder.decode([String: W3sMerchant].self, from: Data(json.utf8))
        let merchant = try #require(map["REG-001"])
        #expect(merchant.topic == topicBytes)
        #expect(merchant.key == keyBytes)
    }

    @Test("Rejects a topic with the wrong byte length")
    func rejectsWrongTopicLength() {
        let badTopic = Data(repeating: 0x01, count: 31).base64URLEncodedString()
        let validKey = Self.validX25519Key().base64URLEncodedString()
        let json = #"{"topic":"\#(badTopic)","key":"\#(validKey)"}"#
        #expect(throws: DecodingError.self) {
            _ = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        }
    }

    @Test("Rejects a key with the wrong byte length", arguments: [31, 33, 65])
    func rejectsWrongKeyLength(badLength: Int) {
        let validTopic = Data(repeating: 0x01, count: 32).base64URLEncodedString()
        let badKey = Data(repeating: 0x02, count: badLength).base64URLEncodedString()
        let json = #"{"topic":"\#(validTopic)","key":"\#(badKey)"}"#
        #expect(throws: DecodingError.self) {
            _ = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        }
    }

    @Test("Rejects invalid Base64URL (contains standard + or /)")
    func rejectsInvalidBase64URL() {
        // 33-byte string that, when standard-base64 encoded, contains '/' — we re-emit
        // it explicitly here to be sure the standard alphabet trips the strict check.
        let invalidKey = "A/" + String(repeating: "A", count: 42)
        let validTopic = Data(repeating: 0x01, count: 32).base64URLEncodedString()
        let json = #"{"topic":"\#(validTopic)","key":"\#(invalidKey)"}"#
        #expect(throws: DecodingError.self) {
            _ = try decoder.decode(W3sMerchant.self, from: Data(json.utf8))
        }
    }
}
