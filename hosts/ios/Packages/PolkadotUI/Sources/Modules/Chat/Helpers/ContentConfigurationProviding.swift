import UIKit

public protocol ContentConfigurationProviding: Hashable {
    func configuration() -> UIContentConfiguration
}

public extension ContentConfigurationProviding {
    func equalTo(_ other: any ContentConfigurationProviding) -> Bool {
        AnyHashable(self) == AnyHashable(other)
    }
}
