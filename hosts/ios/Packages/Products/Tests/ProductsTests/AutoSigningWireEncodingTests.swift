import Foundation
import KeyDerivation
import SubstrateSdk
import Testing
@testable import Products

private final class FixedEntropyManager: RootEntropyManaging {
    private let entropy: Data

    init(entropy: Data) {
        self.entropy = entropy
    }

    func fetchRootEntropy() throws -> Data { entropy }
    func createRootEntropy(_: Data) throws {}
    func hasRootEntropy() throws -> Bool { true }
}

/// The core decodes an `AutoSigning` allocation as `SsoAllocatedResource::AutoSigning`, whose
/// layout `auto_signing_secret_is_fixed_width_on_the_mobile_wire` pins in
/// `rust/crates/truapi/src/host_internal/sso_messages.rs`; the entropy vector is pinned by
/// `product_ring_vrf_domain_entropy_matches_ios_vector` in `host_logic/product_account.rs`.
@Suite("AutoSigning wire encoding")
struct AutoSigningWireEncodingTests {
    /// One `Allocated(AutoSigning)` outcome: list length, variant tags, the 64-byte key
    /// and the 32-byte ring-VRF domain entropy.
    private let outcomesHex = "0x040003"
        + String(repeating: "11", count: 64)
        + String(repeating: "22", count: 32)

    private func outcomes() throws -> [AllocationOutcome] {
        let secrets = try AutoSigningSecrets(
            productRootPrivateKey: Data(repeating: 0x11, count: 64),
            ringVrfDomainEntropy: Data(repeating: 0x22, count: 32)
        )
        return [.allocated(.autoSigning(secrets))]
    }

    @Test("an AutoSigning allocation encodes as the core decodes it")
    func encodesAsTheCore() throws {
        let encoder = ScaleEncoder()
        try outcomes().encode(scaleEncoder: encoder)

        #expect(try encoder.encode() == Data(hexString: outcomesHex))
    }

    @Test("an AutoSigning allocation decodes back to the same secrets")
    func roundTrips() throws {
        let decoder = try ScaleDecoder(data: Data(hexString: outcomesHex))

        #expect(try [AllocationOutcome](scaleDecoder: decoder) == outcomes())
    }

    @Test("secrets without a 32-byte ring-VRF domain entropy are rejected")
    func rejectsShortEntropy() {
        #expect(throws: AutoSigningSecretsError.invalidEntropyLength(expected: 32, actual: 31)) {
            try AutoSigningSecrets(
                productRootPrivateKey: Data(repeating: 0x11, count: 64),
                ringVrfDomainEntropy: Data(repeating: 0x22, count: 31)
            )
        }
    }

    @Test("derived AutoSigning secrets carry the core's ring-VRF domain entropy")
    func derivesTheCoresDomainEntropy() throws {
        let holder = ProductAccountHolder(
            entropyManager: FixedEntropyManager(entropy: Data((1 ... 32).map { UInt8($0) }))
        )
        let expected = try Data(
            hexString: "0xe1f3bf6ace06a2620ab57e03ad2dc8c7765d36f01cafbac7682fdbca565ff80f"
        )

        #expect(try holder.deriveAutoSigningSecrets(for: "dim2.paseo").ringVrfDomainEntropy == expected)
    }
}
