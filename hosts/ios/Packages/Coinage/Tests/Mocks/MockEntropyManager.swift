import Foundation
import KeyDerivation

final class MockEntropyManager: RootEntropyManaging {
    private var entropy: Data?

    init(entropy: Data? = nil) {
        self.entropy = entropy
    }

    func fetchRootEntropy() throws -> Data {
        guard let entropy else {
            throw RootEntropyManagerError.noEntropyFound
        }
        return entropy
    }

    func createRootEntropy(_ entropy: Data) throws {
        self.entropy = entropy
    }

    func hasRootEntropy() throws -> Bool {
        entropy != nil
    }
}
