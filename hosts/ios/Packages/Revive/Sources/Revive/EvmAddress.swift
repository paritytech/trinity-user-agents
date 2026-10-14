import Foundation

/// A pallet-revive (EVM) account address: exactly ``EvmAddressFormat/size`` bytes. It is an alias, so
/// bytes that arrive from outside (remote config, a contract's answer) pass ``EvmAddressFormat/validate(_:)``
/// at the boundary rather than being trusted as-is.
public typealias EvmAddress = Data

public enum EvmAddressError: Error, Equatable {
    case invalidSize(Int)
}

public enum EvmAddressFormat {
    public static let size = 20
    public static let zero = EvmAddress(repeating: 0, count: size)

    public static func validate(_ bytes: Data) throws -> EvmAddress {
        guard bytes.count == size else {
            throw EvmAddressError.invalidSize(bytes.count)
        }

        return bytes
    }
}
