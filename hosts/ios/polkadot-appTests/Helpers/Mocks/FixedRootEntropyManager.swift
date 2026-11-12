import Foundation
import KeyDerivation

final class FixedRootEntropyManager: RootEntropyManaging {
    private var entropy: Data

    init(entropy: Data = Data(repeating: 0x01, count: 16)) {
        self.entropy = entropy
    }

    func fetchRootEntropy() throws -> Data {
        entropy
    }

    func createRootEntropy(_ entropy: Data) throws {
        self.entropy = entropy
    }

    func hasRootEntropy() throws -> Bool {
        true
    }
}
