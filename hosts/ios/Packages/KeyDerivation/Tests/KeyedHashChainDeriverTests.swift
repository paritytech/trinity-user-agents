import Foundation
import KeyDerivation
import SubstrateSdk
import Testing

/// Byte-level pins for the keyed-hash HDKD (ring-VRF and ECDH trees).
/// Vectors are computed for root entropy 0x0102...20 — the cross-platform contract.
@Suite("KeyedHashChain Deriver Tests")
struct KeyedHashChainDeriverTests {
    let rootEntropy = Data((1 ... 32).map { UInt8($0) })

    @Test("Ring-VRF tree root is hash(root_entropy, \"ring-vrf\")")
    func ringVrfRootVector() throws {
        let root = try KeyedHashChainDeriver.deriveRoot(
            entropy: rootEntropy,
            rootKey: Data("ring-vrf".utf8)
        )

        let expected = try Data(
            hexString: "0x372b08255c7798fe3193756296005adc4c44adb9f3986fb718aa98a48b4bf725"
        )
        #expect(root == expected)
    }

    @Test("Full and light personhood entropies match the //peopl.dot//index vectors")
    func personhoodEntropyVectors() throws {
        let fullPerson = RingVrfEntropyDeriver(domain: "peopl.dot", index: 0)
        let litePerson = RingVrfEntropyDeriver(domain: "peopl.dot", index: 1)

        let expectedFull = try Data(
            hexString: "0xc47086f94a7f4c05b7afd9f2339d3fea168f3823b5424ba1f7b31043d8ef60af"
        )
        let expectedLite = try Data(
            hexString: "0x8d7f5e1510a7e8d813887e100f5a260ec9de60e68695477b93360ee7e3d16a9f"
        )

        #expect(try fullPerson.deriveEntropy(from: rootEntropy) == expectedFull)
        #expect(try litePerson.deriveEntropy(from: rootEntropy) == expectedLite)
    }

    @Test("ECDH key material matches the //{domain} vectors for chat, sso, and game")
    func ecdhDomainVectors() throws {
        let deriver = EcdhKeyMaterialDeriver(entropyManager: StubEntropyManager(entropy: rootEntropy))

        let expected = [
            "chat": "0x88705746a9788fb8bc232cc463b243a2828e470b932ce60a16426cc79b4618b1",
            "sso": "0xc2c0f3112da0aa390a7d7b644a7f073b80782e49a3240685bc071046d6cc8ad3",
            "game": "0x3267d8a02b51d2ada4bfdd56a5f503972a6fac179d9450cf96b03002d202dfc8"
        ]

        for (domain, hex) in expected {
            #expect(try deriver.deriveKeyMaterial(for: domain) == Data(hexString: hex))
        }
    }

    @Test("hash(data, key) argument order is pinned — swapping arguments changes the result")
    func keyedHashArgumentOrder() throws {
        let straight = try KeyedHashChainDeriver.deriveRoot(
            entropy: rootEntropy,
            rootKey: Data("ring-vrf".utf8)
        )
        let swapped = try KeyedHashChainDeriver.deriveRoot(
            entropy: Data("ring-vrf".utf8),
            rootKey: rootEntropy
        )

        #expect(straight != swapped)
    }

    @Test("String segments use the standard substrate chain-code normalization")
    func stringSegmentNormalization() throws {
        // SCALE("peopl.dot") = compact(9) ++ utf8, zero-padded to 32 bytes.
        let expected = try Data(
            hexString: "0x2470656f706c2e646f7400000000000000000000000000000000000000000000"
        )

        let derived = try KeyedHashChainDeriver.derive(
            parent: rootEntropy,
            segments: [.string("peopl.dot")]
        )

        #expect(try derived == rootEntropy.blake2b32WithKey(expected))
    }

    @Test("Empty and separator-containing segments are rejected")
    func invalidSegmentsThrow() throws {
        for segment in ["", "a/b", "a//b"] {
            #expect(throws: KeyedHashChainError.invalidSegment(segment)) {
                _ = try KeyedHashChainDeriver.derive(parent: rootEntropy, segments: [.string(segment)])
            }
        }
    }
}

private struct StubEntropyManager: RootEntropyManaging {
    let entropy: Data

    func fetchRootEntropy() throws -> Data { entropy }
    func createRootEntropy(_: Data) throws {}
    func hasRootEntropy() throws -> Bool { true }
}
