import Foundation

public enum AssetTypeError: Error {
    case unexpectedType(String?)
}

public enum AssetType: String {
    case native
    case statemine
    case orml
    case ormlHydrationEvm = "orml-hydration-evm"

    public init?(rawType: String?) {
        if let rawType {
            self.init(rawValue: rawType)
        } else {
            self = .native
        }
    }

    public static func createOrError(from rawType: String?) throws -> AssetType {
        guard let assetType = AssetType(rawType: rawType) else {
            throw AssetTypeError.unexpectedType(rawType)
        }

        return assetType
    }
}
