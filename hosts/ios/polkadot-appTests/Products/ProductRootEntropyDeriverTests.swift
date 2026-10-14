import Foundation
import KeyDerivation
import Keystore_iOS
import Products
import Testing

@testable import polkadot_app

@Suite("ProductRootEntropyDeriver Tests")
struct ProductRootEntropyDeriverTests {
    private let testRootEntropy = Data(repeating: 0xAB, count: 16)

    private func makeSUT() throws -> ProductRootEntropyDeriver {
        let manager = RootEntropyManager(
            keychain: InMemoryKeychain(),
            installationKeyIdStore: MockInstallationKeyIdStore()
        )
        try manager.createRootEntropy(testRootEntropy)
        return ProductRootEntropyDeriver(entropyManager: manager)
    }

    @Test("derives entropy matching reference implementation")
    func derivesEntropyMatchingReferenceImplementation() throws {
        let sut = try makeSUT()
        let key = Data("my-key".utf8)

        let result = try sut.deriveEntropy(productId: "test.product.dot", key: key)

        let expected = try Data(hexString: "0x479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a")
        #expect(result == expected)
    }

    @Test("different key produces different entropy")
    func differentKeyProducesDifferentEntropy() throws {
        let sut = try makeSUT()
        let key = Data("other-key".utf8)

        let result = try sut.deriveEntropy(productId: "test.product.dot", key: key)

        let expected = try Data(hexString: "0x0d576d5d77cb179bf94b85cb1d644b7879315e74d9e69791fb9cbe94df3c7c39")
        #expect(result == expected)
    }

    @Test("different product produces different entropy")
    func differentProductProducesDifferentEntropy() throws {
        let sut = try makeSUT()
        let key = Data("my-key".utf8)

        let result = try sut.deriveEntropy(productId: "other.product.dot", key: key)

        let expected = try Data(hexString: "0xe2f25271c106593c2977d5965f52fa1d2227da0fc110d682c8cb8f30b2ba21c8")
        #expect(result == expected)
    }

    @Test("same inputs produce same output (determinism)")
    func sameInputsProduceSameOutput() throws {
        let sut = try makeSUT()
        let key = Data("my-key".utf8)

        let first = try sut.deriveEntropy(productId: "test.product.dot", key: key)
        let second = try sut.deriveEntropy(productId: "test.product.dot", key: key)

        #expect(first == second)
    }

    @Test("rejects key longer than 32 bytes")
    func rejectsKeyLongerThan32Bytes() throws {
        let sut = try makeSUT()
        let key = Data(repeating: 0x01, count: 33)

        #expect(throws: ProductRootEntropyDeriverError.self) {
            _ = try sut.deriveEntropy(productId: "test.product.dot", key: key)
        }
    }

    @Test("output is 32 bytes")
    func outputIs32Bytes() throws {
        let sut = try makeSUT()
        let key = Data("test".utf8)

        let result = try sut.deriveEntropy(productId: "test.product.dot", key: key)

        #expect(result.count == 32)
    }
}
