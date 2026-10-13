import Testing
import Foundation
@testable import polkadot_app
import KeyDerivation
import Keystore_iOS
import Products
import SubstrateSdk

/// Covers the fix that replaced the hardcoded `.dot` product TLD with the network-resolved, cached
/// DotNs TLD across built-in account and personhood-key derivation.
@Suite("DotNs TLD resolution")
struct DotNsTldResolutionTests {
    /// `currentTldOrError()` is the guard that stops derivation against an unknown TLD: it returns
    /// the cached value when present and throws otherwise, instead of silently assuming `.dot`.
    @Test
    func currentTldOrErrorReturnsCachedValueOrThrows() throws {
        #expect(try StubDotNsTldProvider(tld: "ksm").currentTldOrError() == "ksm")
        #expect(throws: DotNsTldError.self) {
            try StubDotNsTldProvider(tld: nil).currentTldOrError()
        }
    }

    /// The repositories must refuse to vend TLD-dependent wallets/keys when no TLD is cached (the
    /// post-onboarding invariant), while TLD-independent pallet-context wallets stay available.
    @Test
    func repositoriesThrowForTldDependentAccessorsWhenTldMissing() throws {
        let walletRepo = WalletManagerRepository(tldProvider: StubDotNsTldProvider(tld: nil))
        #expect(throws: DotNsTldError.self) { try walletRepo.main() }
        #expect(throws: DotNsTldError.self) { try walletRepo.depositWallet() }
        // TLD-independent accessors derive from fixed pallet-context paths and never fail.
        _ = walletRepo.resourcesAlias()

        let vrfRepo = BandersnatchManagerRepository(tldProvider: StubDotNsTldProvider(tld: nil))
        #expect(throws: DotNsTldError.self) { try vrfRepo.fullPerson() }
        #expect(throws: DotNsTldError.self) { try vrfRepo.litePerson() }
    }

    /// The bug fix itself: product domains and derivation paths must embed the resolved TLD, not a
    /// hardcoded `.dot`. Parameterized so a reverted fix (constant `.dot`) fails on any non-dot TLD.
    @Test(arguments: ["dot", "ksm", "paseo"])
    func derivationPathsEmbedResolvedTld(_ tld: String) {
        #expect(BuiltInProduct.personhood(for: tld) == "peopl.\(tld)")
        #expect(BuiltInProduct.dim2(for: tld) == "dim2.\(tld)")
        #expect(BuiltInProduct.uid(for: tld) == "uid.\(tld)")
        #expect(WalletDerivationPath.main(for: tld).contains("uid.\(tld)"))
    }

    /// End-to-end proof that the TLD flows into real key material: two TLDs over the same root
    /// entropy derive different main accounts and different personhood member keys. Also exercises
    /// the shared repository mocks.
    @Test
    func differentTldsDeriveDifferentKeyMaterial() throws {
        let entropyManager = try makeEntropyManager()

        let dotMain = try MockWalletManagerRepository(tld: "dot", entropyManager: entropyManager)
            .main().getRawPublicKey()
        let ksmMain = try MockWalletManagerRepository(tld: "ksm", entropyManager: entropyManager)
            .main().getRawPublicKey()
        #expect(dotMain != ksmMain)

        let dotMember = try MockBandersnatchManagerRepository(tld: "dot", entropyManager: entropyManager)
            .fullPerson().getMemberKey()
        let ksmMember = try MockBandersnatchManagerRepository(tld: "ksm", entropyManager: entropyManager)
            .fullPerson().getMemberKey()
        #expect(dotMember != ksmMember)
    }
}

private extension DotNsTldResolutionTests {
    func makeEntropyManager() throws -> RootEntropyManaging {
        let manager = RootEntropyManager(
            keychain: InMemoryKeychain(),
            installationKeyIdStore: MockInstallationKeyIdStore()
        )
        try manager.createRootEntropy(Data.randomOrError(of: 32))
        return manager
    }
}
