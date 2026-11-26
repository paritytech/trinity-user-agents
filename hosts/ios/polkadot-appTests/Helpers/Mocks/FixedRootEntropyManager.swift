import Foundation
import KeyDerivation

final class FixedRootEntropyManager: RootEntropyManaging {
    private var entropy: Data
    private let onFetch: () throws -> Void

    init(entropy: Data = Data(repeating: 0x01, count: 16), onFetch: @escaping () throws -> Void = {}) {
        self.entropy = entropy
        self.onFetch = onFetch
    }

    func fetchRootEntropy() throws -> Data {
        try onFetch()
        return entropy
    }

    func createRootEntropy(_ entropy: Data) throws {
        self.entropy = entropy
    }

    func hasRootEntropy() throws -> Bool {
        true
    }
}
