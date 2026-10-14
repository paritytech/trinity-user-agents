import Foundation
import Revive

public enum AccountDataStoreAbiError: Error {
    case decodingFailed(function: String)
}

/// ABI of the `AccountDataStore` contract (`account-data-store-contract`, `abi/AccountDataStore.json`).
public enum AccountDataStoreAbi {
    // registerCoinageInstallation(bytes record)
    private static let registerFunction = EvmAbi.Function(
        name: "registerCoinageInstallation",
        inputs: [.dynamicBytes],
        outputs: []
    )

    // getCoinageInstallations(address owner) returns (bytes[])
    private static let getFunction = EvmAbi.Function(
        name: "getCoinageInstallations",
        inputs: [.address],
        outputs: [.array(.dynamicBytes)]
    )

    public static func encodeRegisterInstallation(record: Data) throws -> Data {
        try EvmAbi.encode(registerFunction, parameters: [record])
    }

    public static func encodeGetInstallations(owner: EvmAddress) throws -> Data {
        try EvmAbi.encode(getFunction, parameters: [owner])
    }

    /// Throws rather than reporting an empty list: an unreadable answer is not evidence that the seed
    /// has no installations registered, and treating it as one would skip recovery and re-register.
    public static func decodeGetInstallations(output: Data) throws -> [Data] {
        guard
            let decoded = try? EvmAbi.decode(getFunction, output: output),
            let records = decoded.first as? [Data]
        else {
            throw AccountDataStoreAbiError.decodingFailed(function: getFunction.name)
        }
        return records
    }
}
