import DurableTransactions
import Foundation
import Revive
import SubstrateSdk

public extension TxDomainId {
    /// Registrations of this seed's installations in the `AccountDataStore` contract.
    static let coinageInstallation = TxDomainId("coinage-installation")
}

/// One installation registered in one contract. The group is the whole history of registering it
/// there: naming both keeps the oracle reading the contract an attempt actually wrote to, and makes a
/// contract change start a registration of its own.
public struct InstallationRegistrationTarget: Hashable, Sendable {
    static let groupSeparator: Character = "/"
    private static let logIdBytes = 4

    public let contract: EvmAddress
    public let installation: CoinageInstallationId

    public init(contract: EvmAddress, installation: CoinageInstallationId) {
        self.contract = contract
        self.installation = installation
    }

    public init?(groupId: DurableTxGroupId) {
        let parts = groupId.split(separator: Self.groupSeparator)
        guard parts.count == 2,
              let contract = try? EvmAddressFormat.validate(Data(hexString: String(parts[0]))),
              let installation = try? CoinageInstallationId(hex: String(parts[1]))
        else { return nil }
        self.init(contract: contract, installation: installation)
    }

    public var registrationGroup: DurableTxGroupId {
        "\(contract.toHex(includePrefix: true))\(Self.groupSeparator)\(installation.pageSegment)"
    }

    var logDescription: String {
        "installation=\(installation.logId) contract=\(contract.toHex(includePrefix: true))"
    }
}

public extension CoinageInstallationId {
    /// Enough to tell installations apart in a shared log, not enough to stand in for the id.
    var logId: String {
        value.prefix(4).toHex(includePrefix: true) + "…"
    }
}
