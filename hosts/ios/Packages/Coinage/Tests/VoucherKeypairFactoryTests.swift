import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import KeyDerivation
import BandersnatchApi
@testable import Coinage

struct VoucherEntropyDerivingTests {
    private let seed = Data(repeating: 0x01, count: 32)

    @Test("Derives entropy successfully for a valid all-hard voucher path")
    func derivesEntropyValidPath() throws {
        let deriver = VoucherEntropyDeriving(path: "//coinage-ring-vrf//4294967295//0x0101//1")
        let entropy = try deriver.deriveEntropy(from: seed)

        #expect(entropy.count == 32)
    }

    @Test("Throws an error when path contains soft junctions")
    func derivesEntropyInvalidPath() throws {
        // Ring-VRF entropy derivation has no soft variant — every junction must be hard.
        let deriver = VoucherEntropyDeriving(path: "//coinage-ring-vrf//4294967295//0x0101/1")

        #expect(throws: VoucherEntropyDerivingError.invalidDerivationPath) {
            try deriver.deriveEntropy(from: seed)
        }
    }

    @Test("Throws an error for invalid junction format")
    func derivesEntropyInvalidFormat() throws {
        let deriver = VoucherEntropyDeriving(path: "invalid")

        // SubstrateJunctionFactory throws for paths not starting with / or //
        #expect(throws: Error.self) {
            try deriver.deriveEntropy(from: seed)
        }
    }
}

struct VoucherKeypairFactoryTests {
    private let mockEntropyManager: MockEntropyManager
    private let factory: VoucherKeypairFactory

    init() {
        mockEntropyManager = MockEntropyManager()
        factory = VoucherKeypairFactory(entropyManager: mockEntropyManager)
    }

    @Test("Successfully creates public key when entropy is present")
    func derivePublicKeySuccess() throws {
        let validEntropy = Data(repeating: 0x01, count: 32)
        try mockEntropyManager.createRootEntropy(validEntropy)

        let publicKey = try factory.derivePublicKey(index: 1)

        #expect(!publicKey.isEmpty)
    }

    @Test("Throws error when entropy is missing")
    func derivePublicKeyMissingEntropy() throws {
        #expect(throws: RootEntropyManagerError.noEntropyFound) {
            try factory.derivePublicKey(index: 1)
        }
    }

    @Test("Derives deterministic keys for same entropy and index")
    func deterministicDerivation() throws {
        let entropy = Data(repeating: 0xAB, count: 32)
        try mockEntropyManager.createRootEntropy(entropy)

        let manager2 = MockEntropyManager(entropy: entropy)
        let factory2 = VoucherKeypairFactory(entropyManager: manager2)

        let key1 = try factory.derivePublicKey(index: 10)
        let key2 = try factory2.derivePublicKey(index: 10)

        #expect(key1 == key2)
    }

    @Test("Derives different keys for different indices")
    func differentIndicesProduceDifferentKeys() throws {
        let entropy = Data(repeating: 0xAB, count: 32)
        try mockEntropyManager.createRootEntropy(entropy)

        let key1 = try factory.derivePublicKey(index: 1)
        let key2 = try factory.derivePublicKey(index: 2)

        #expect(key1 != key2)
    }

    @Test("Successfully creates a key manager")
    func createKeyManagerSuccess() throws {
        let manager: BandersnatchKeyManaging? = try factory.createKeyManager(index: 5)
        #expect(manager != nil)
    }

    @Test("The path names the installation as the page and the item as a hard junction")
    func derivationPathCorrectness() {
        let page = CoinageInstallationId.test.pageSegment
        #expect(factory.voucherPath(for: 123) == "//coinage-ring-vrf//4294967295//\(page)//123")
    }

    @Test("A voucher derives along its installation's page")
    func derivesAlongPage() throws {
        let seed = Data(repeating: 0x01, count: 32)
        let path = "//coinage-ring-vrf//4294967295//\(CoinageInstallationId.test.pageSegment)//7"
        let expected = try SubstrateJunctionFactory().parse(path: path).chaincodes.reduce(seed) { entropy, chaincode in
            try entropy.blake2b32WithKey(chaincode.data)
        }

        let derived = try VoucherEntropyDeriving(path: factory.voucherPath(for: 7)).deriveEntropy(from: seed)

        #expect(derived == expected)
    }

    @Test("The same item under two installations is two keys")
    func installationsDiffer() throws {
        try mockEntropyManager.createRootEntropy(Data(repeating: 0xAB, count: 32))

        let current = try factory.derivePublicKey(index: CoinageKeyIndex(installation: .test, item: 7))
        let other = try factory.derivePublicKey(index: CoinageKeyIndex(installation: .other, item: 7))

        #expect(current != other)
    }
}
