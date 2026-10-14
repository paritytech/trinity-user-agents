import Foundation
import SubstrateSdk

public enum NameHash {
    /// ENS namehash: split by `.`, reverse labels,
    /// iteratively `keccak256(node + keccak256(label))`.
    /// Returns 32 zero bytes for empty string.
    public static func nameHash(_ name: String) throws -> Data {
        guard !name.isEmpty else {
            return Data(repeating: 0, count: 32)
        }

        var node = Data(repeating: 0, count: 32)
        let labels = name.split(separator: ".")

        for label in labels.reversed() {
            let labelHash = try Data(label.utf8).keccak256()
            node = try (node + labelHash).keccak256()
        }

        return node
    }
}
