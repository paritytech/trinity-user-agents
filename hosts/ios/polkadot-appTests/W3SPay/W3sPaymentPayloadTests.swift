import Foundation
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("W3sPaymentPayload + W3sPaymentEnvelope")
struct W3sPaymentEncodingTests {
    @Test("Payload SCALE-encodes deterministically and is shape-checkable")
    func payloadEncoding() throws {
        let payload = W3sPaymentPayload(
            amount: "9.00",
            timestampMs: 1_700_000_000_000,
            coins: [Data(repeating: 0x01, count: 32), Data(repeating: 0x02, count: 32)],
            paymentId: "REG-001/42"
        )

        let encoded = try payload.scaleEncoded()
        // Deterministic: two encodings of the same struct produce the same bytes.
        let encodedAgain = try payload.scaleEncoded()
        #expect(encoded == encodedAgain)

        // The amount string + paymentId travel as length-prefixed UTF-8 — both substrings
        // appear in the encoded bytes.
        #expect(encoded.contains(Data("9.00".utf8)))
        #expect(encoded.contains(Data("REG-001/42".utf8)))

        // 32-byte coin keys appear unchanged inside the encoded SCALE Vec<Vec<u8>>.
        #expect(encoded.contains(Data(repeating: 0x01, count: 32)))
        #expect(encoded.contains(Data(repeating: 0x02, count: 32)))
    }

    @Test("Envelope encodes ciphertext length-prefixed and pubkey raw, matching AppHandshakeData layout")
    func envelopeEncoding() throws {
        let ciphertext = Data((0 ..< 48).map { UInt8($0) }) // 48 bytes = nonce(12) + ct(20) + tag(16)
        let pubKey = Data(repeating: 0xAB, count: 32) // 32-byte X25519 key

        let envelope = W3sPaymentEnvelope(
            encryptedData: ciphertext,
            ephemeralPublicKey: pubKey
        )
        let encoded = try envelope.scaleEncoded()

        // The encoded form is: compact-length(ciphertext) ‖ ciphertext ‖ pubKey (raw, 32 bytes).
        // For a 48-byte ciphertext the SCALE compact prefix is single-byte 0xC0 (0b1100_0000),
        // followed by the 48 bytes, followed by the 32-byte pubkey: total 1 + 48 + 32 = 81.
        #expect(encoded.count == 1 + ciphertext.count + pubKey.count)
        #expect(encoded.suffix(pubKey.count) == pubKey)
        #expect(encoded.dropFirst().prefix(ciphertext.count) == ciphertext)
    }

    @Test("Payload encodes to exact golden bytes (regression guard for field order / encoding)")
    func payloadGoldenBytes() throws {
        // Minimal payload chosen so every byte can be reasoned about by hand:
        //   amount       = "9.00"     → SCALE String: compact(4)=0x10, then UTF-8 0x39 2E 30 30
        //   timestampMs  = 1_000      → 8-byte little-endian: E8 03 00 00 00 00 00 00
        //   coins        = []         → SCALE Vec<u8> outer length: compact(0)=0x00
        //   paymentId    = "x"        → SCALE String: compact(1)=0x04, then UTF-8 0x78
        let payload = W3sPaymentPayload(
            amount: "9.00",
            timestampMs: 1_000,
            coins: [],
            paymentId: "x"
        )
        let encoded = try payload.scaleEncoded()
        let expected = Data([
            // amount
            0x10, 0x39, 0x2E, 0x30, 0x30,
            // timestampMs (little-endian)
            0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // coins — outer compact-length only (empty vec)
            0x00,
            // paymentId
            0x04, 0x78
        ])
        #expect(encoded == expected)
    }

    @Test("Envelope encodes to exact golden bytes (compact-prefixed ct + raw 65-byte pubkey)")
    func envelopeGoldenBytes() throws {
        // encryptedData = 0xAB → SCALE Vec<u8>: compact(1)=0x04, then 0xAB
        // ephemeralPublicKey = 65 raw bytes: 0x04 + 0x01..0x40
        let pubKey = Data([0x04]) + Data((1 ... 64).map { UInt8($0) })
        let envelope = W3sPaymentEnvelope(
            encryptedData: Data([0xAB]),
            ephemeralPublicKey: pubKey
        )
        let encoded = try envelope.scaleEncoded()
        let expected = Data([0x04, 0xAB]) + pubKey
        #expect(encoded == expected)
    }
}
