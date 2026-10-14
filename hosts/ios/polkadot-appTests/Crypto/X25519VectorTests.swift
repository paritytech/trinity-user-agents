import CryptoKit
import Foundation
import SubstrateSdk
import Testing

/// RFC 7748 X25519 test vectors (§5.2 function vectors, §6.1 Diffie-Hellman vectors).
/// Pins the platform implementation the whole encryption stack relies on.
@Suite("X25519 RFC 7748 vectors")
struct X25519VectorTests {
    private func x25519(scalar: String, uCoordinate: String) throws -> Data {
        let privateKey = try Curve25519.KeyAgreement.PrivateKey(
            rawRepresentation: Data(hexString: scalar)
        )
        let publicKey = try Curve25519.KeyAgreement.PublicKey(
            rawRepresentation: Data(hexString: uCoordinate)
        )
        return try privateKey.sharedSecretFromKeyAgreement(with: publicKey)
            .withUnsafeBytes { Data($0) }
    }

    @Test("§5.2 function vector 1")
    func functionVector1() throws {
        let output = try x25519(
            scalar: "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",
            uCoordinate: "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c"
        )
        #expect(output.toHex() == "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552")
    }

    @Test("§5.2 function vector 2")
    func functionVector2() throws {
        let output = try x25519(
            scalar: "4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d",
            uCoordinate: "e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493"
        )
        #expect(output.toHex() == "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957")
    }

    @Test("§5.2 iterated vector, 1,000 iterations")
    func iteratedVector() throws {
        var scalar = try Data(hexString: "0900000000000000000000000000000000000000000000000000000000000000")
        var uCoordinate = scalar

        for _ in 0 ..< 1_000 {
            let output = try x25519(scalar: scalar.toHex(), uCoordinate: uCoordinate.toHex())
            uCoordinate = scalar
            scalar = output
        }

        #expect(scalar.toHex() == "684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51")
    }

    @Test("§6.1 Diffie-Hellman: public keys derive from the private scalars")
    func publicKeyDerivation() throws {
        let alicePrivate = try Curve25519.KeyAgreement.PrivateKey(
            rawRepresentation: Data(hexString: "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
        )
        let bobPrivate = try Curve25519.KeyAgreement.PrivateKey(
            rawRepresentation: Data(hexString: "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb")
        )

        #expect(
            alicePrivate.publicKey.rawRepresentation.toHex() ==
                "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        )
        #expect(
            bobPrivate.publicKey.rawRepresentation.toHex() ==
                "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
        )
    }

    @Test("§6.1 Diffie-Hellman: both sides derive the shared secret K")
    func sharedSecret() throws {
        let expected = "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"

        let aliceSide = try x25519(
            scalar: "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
            uCoordinate: "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
        )
        let bobSide = try x25519(
            scalar: "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
            uCoordinate: "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        )

        #expect(aliceSide.toHex() == expected)
        #expect(bobSide.toHex() == expected)
    }
}
