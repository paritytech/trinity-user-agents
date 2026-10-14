import Foundation

public struct BlurHash: Equatable, Hashable {
    public let value: String

    public init?(_ value: String) {
        guard value.count >= 6,
              value.allSatisfy({ BlurHashMath.decodeCharacters[$0] != nil }),
              let firstCharacter = value.first,
              let sizeFlag = BlurHashMath.decodeCharacters[firstCharacter] else {
            return nil
        }

        let componentCount = ((sizeFlag % 9) + 1) * ((sizeFlag / 9) + 1)
        guard value.count == 4 + 2 * componentCount else { return nil }

        self.value = value
    }

    public init?(rawValue: Data) {
        guard let value = String(data: rawValue, encoding: .utf8) else { return nil }
        self.init(value)
    }

    public func toData() -> Data {
        Data(value.utf8)
    }
}
