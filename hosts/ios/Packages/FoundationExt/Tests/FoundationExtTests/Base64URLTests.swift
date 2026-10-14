import Foundation
import Testing
@testable import FoundationExt

@Suite("Data+Base64URL")
struct Base64URLTests {
    @Test("Encodes RFC 4648 §5 test vectors")
    func encodesKnownVectors() {
        #expect(Data("".utf8).base64URLEncodedString() == "")
        #expect(Data("f".utf8).base64URLEncodedString() == "Zg")
        #expect(Data("fo".utf8).base64URLEncodedString() == "Zm8")
        #expect(Data("foo".utf8).base64URLEncodedString() == "Zm9v")
        #expect(Data("foob".utf8).base64URLEncodedString() == "Zm9vYg")
        #expect(Data("fooba".utf8).base64URLEncodedString() == "Zm9vYmE")
        #expect(Data("foobar".utf8).base64URLEncodedString() == "Zm9vYmFy")
    }

    @Test("Uses URL-safe alphabet (- and _ replace + and /)")
    func usesURLSafeAlphabet() {
        // Bytes that hit + and / in standard base64.
        let bytes = Data([0xFB, 0xFF, 0xFE, 0xFB, 0xFF, 0xFE])
        let encoded = bytes.base64URLEncodedString()
        #expect(!encoded.contains("+"))
        #expect(!encoded.contains("/"))
        #expect(!encoded.contains("="))
        #expect(encoded.contains("-") || encoded.contains("_"))
    }

    @Test("Round-trips random byte sequences across pad-counts")
    func roundTripsAllLengths() {
        for length in 0 ..< 32 {
            var bytes = Data(count: length)
            for index in 0 ..< length {
                bytes[index] = UInt8(truncatingIfNeeded: index &* 31 &+ 7)
            }
            let decoded = Data(base64URLEncoded: bytes.base64URLEncodedString())
            #expect(decoded == bytes, "length \(length) failed to round-trip")
        }
    }

    @Test("Decodes valid Base64URL inputs (with and without padding)")
    func decodesValidInputs() {
        #expect(Data(base64URLEncoded: "Zm9vYmFy") == Data("foobar".utf8))
        #expect(Data(base64URLEncoded: "Zm9vYmE") == Data("fooba".utf8))
        // Re-padded input also accepted, for clients that include "=".
        #expect(Data(base64URLEncoded: "Zm9vYmE=") == Data("fooba".utf8))
    }

    @Test("Rejects strings containing standard-base64 sentinels")
    func rejectsStandardSentinels() {
        // A Base64URL decoder shouldn't accept the + and / characters at all.
        #expect(Data(base64URLEncoded: "abc+") == nil)
        #expect(Data(base64URLEncoded: "ab/c") == nil)
    }
}
