import Foundation
import KeyDerivation
import SubstrateSdk
import Testing
@testable import Products

private final class MockEntropyManager: RootEntropyManaging {
    private let entropy: Data

    init(entropy: Data) {
        self.entropy = entropy
    }

    func fetchRootEntropy() throws -> Data { entropy }
    func createRootEntropy(_: Data) throws {}
    func hasRootEntropy() throws -> Bool { true }
}

/// Vectors shared with Android `DeriveEntropyUseCaseTest` to guarantee
/// cross-platform derivation compatibility.
struct ProductRootEntropyDeriverTests {
    private let testRootEntropy = Data(repeating: 0xAB, count: 16)

    private func makeDeriver() -> ProductRootEntropyDeriver {
        ProductRootEntropyDeriver(entropyManager: MockEntropyManager(entropy: testRootEntropy))
    }

    @Test func derivesEntropyMatchingReferenceImplementation() throws {
        let result = try makeDeriver().deriveEntropy(
            productId: "test.product.dot",
            key: Data("my-key".utf8)
        )

        let expected = try Data(hexString: "479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a")
        #expect(result == expected)
    }

    @Test func differentKeyProducesDifferentEntropy() throws {
        let result = try makeDeriver().deriveEntropy(
            productId: "test.product.dot",
            key: Data("other-key".utf8)
        )

        let expected = try Data(hexString: "0d576d5d77cb179bf94b85cb1d644b7879315e74d9e69791fb9cbe94df3c7c39")
        #expect(result == expected)
    }

    @Test func differentProductProducesDifferentEntropy() throws {
        let result = try makeDeriver().deriveEntropy(
            productId: "other.product.dot",
            key: Data("my-key".utf8)
        )

        let expected = try Data(hexString: "e2f25271c106593c2977d5965f52fa1d2227da0fc110d682c8cb8f30b2ba21c8")
        #expect(result == expected)
    }

    @Test func sameInputsProduceSameOutput() throws {
        let deriver = makeDeriver()

        let result1 = try deriver.deriveEntropy(productId: "test.product.dot", key: Data("my-key".utf8))
        let result2 = try deriver.deriveEntropy(productId: "test.product.dot", key: Data("my-key".utf8))

        #expect(result1 == result2)
    }

    @Test func rejectsKeyLongerThan32Bytes() throws {
        let key = Data(repeating: 0x01, count: 33)

        #expect(throws: ProductRootEntropyDeriverError.keyTooLarge(maxSize: 32, actualSize: 33)) {
            try makeDeriver().deriveEntropy(productId: "test.product.dot", key: key)
        }
    }

    @Test func outputIs32Bytes() throws {
        let result = try makeDeriver().deriveEntropy(
            productId: "test.product.dot",
            key: Data("test".utf8)
        )

        #expect(result.count == 32)
    }
}
