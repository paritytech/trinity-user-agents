import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import KeyDerivation
@testable import Coinage

struct CoinKeypairFactoryTests {
    private let mockEntropyManager: MockEntropyManager
    private let factory: CoinKeypairFactory

    init() {
        mockEntropyManager = MockEntropyManager()
        factory = CoinKeypairFactory(entropyManager: mockEntropyManager)
    }

    @Test("Successfully creates public key when entropy is present")
    func derivePublicKeySuccess() throws {
        let validEntropy = Data(repeating: 0x01, count: 32)
        try mockEntropyManager.createRootEntropy(validEntropy)

        let publicKey = try factory.derivePublicKey(index: 1)

        #expect(publicKey.count == 32)
    }

    @Test("Throws error when entropy is missing")
    func derivePublicKeyMissingEntropy() throws {
        #expect(throws: RootEntropyManagerError.noEntropyFound) {
            _ = try factory.derivePublicKey(index: 1)
        }
    }

    @Test("Derives deterministic keys for same entropy and index")
    func deterministicDerivation() throws {
        let entropy = Data(repeating: 0xAB, count: 32)
        try mockEntropyManager.createRootEntropy(entropy)

        let manager2 = MockEntropyManager(entropy: entropy)
        let factory2 = CoinKeypairFactory(entropyManager: manager2)

        let key1 = try factory.derivePublicKey(index: 5)
        let key2 = try factory2.derivePublicKey(index: 5)

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

    @Test("The path names the installation as the page and the item as a soft junction")
    func derivationPathCorrectness() {
        let page = CoinageInstallationId.test.pageSegment
        #expect(factory.coinPath(for: 123) == "//coinage//4294967295//\(page)/123")
    }

    @Test("An installation page is used as the chain code unchanged")
    func pageIsChainCode() throws {
        let chaincodes = try SubstrateJunctionFactory().parse(path: "//" + CoinageInstallationId.test.pageSegment)
            .chaincodes

        #expect(chaincodes.map(\.data) == [CoinageInstallationId.test.value])
        #expect(chaincodes.map(\.type) == [.hard])
    }

    @Test("The same item under two installations is two keys")
    func installationsDiffer() throws {
        try mockEntropyManager.createRootEntropy(Data(repeating: 0xAB, count: 32))

        let current = try factory.derivePublicKey(index: CoinageKeyIndex(installation: .test, item: 7))
        let other = try factory.derivePublicKey(index: CoinageKeyIndex(installation: .other, item: 7))

        #expect(current != other)
    }
}
