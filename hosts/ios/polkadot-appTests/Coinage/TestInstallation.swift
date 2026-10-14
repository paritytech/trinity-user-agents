import Coinage
import Foundation

extension CoinageInstallationId {
    static let test = fixed(0x5A)
    static let other = fixed(0x11)

    static func fixed(_ byte: UInt8) -> CoinageInstallationId {
        // Always the right size, so the throwing initializer cannot fail here.
        // swiftlint:disable:next force_try
        try! CoinageInstallationId(value: Data(repeating: byte, count: CoinageInstallationId.sizeBytes))
    }
}

/// Lets the suites write an item as a plain number: it names that item in the test installation.
extension CoinageKeyIndex: ExpressibleByIntegerLiteral {
    public init(integerLiteral value: UInt64) {
        self.init(installation: .test, item: value)
    }
}
