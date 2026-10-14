import BigInt
import Foundation
@preconcurrency import SubstrateSdk

public enum RevivePallet {
    public static let name = "Revive"
    public static let callName = "call"

    /// Which name the runtime gives the weight limit of `Revive.call`: pallet-revive renamed `gas_limit`
    /// to `weight_limit` once `gas` came to mean Ethereum gas, and both runtimes are live.
    public enum WeightLimitArgument: String, Sendable {
        case weightLimit = "weight_limit"
        case gasLimit = "gas_limit"
    }

    /// `Revive.call`: runs `data` against the contract at `dest` from the signed origin.
    public struct Call: Sendable {
        public let dest: EvmAddress
        public let value: BigUInt
        public let weightLimit: Substrate.WeightV2
        public let storageDepositLimit: BigUInt
        public let data: Data

        public init(
            dest: EvmAddress,
            value: BigUInt,
            weightLimit: Substrate.WeightV2,
            storageDepositLimit: BigUInt,
            data: Data
        ) {
            self.dest = dest
            self.value = value
            self.weightLimit = weightLimit
            self.storageDepositLimit = storageDepositLimit
            self.data = data
        }

        /// Adds the call under whichever argument name the runtime uses.
        public func add<Builder: ExtrinsicBuilderProtocol>(
            to builder: Builder,
            weightLimitArgument: WeightLimitArgument
        ) throws -> Builder {
            switch weightLimitArgument {
            case .weightLimit:
                try builder.adding(call: runtimeCall(args: CallArgs(self)))
            case .gasLimit:
                try builder.adding(call: runtimeCall(args: LegacyCallArgs(self)))
            }
        }

        private func runtimeCall<Args: Codable>(args: Args) -> RuntimeCall<Args> {
            RuntimeCall(moduleName: RevivePallet.name, callName: RevivePallet.callName, args: args)
        }
    }

    /// `Revive.call` arguments on runtimes that name the weight limit `weight_limit`.
    public struct CallArgs: Codable {
        @BytesCodable public var dest: EvmAddress
        @StringCodable public var value: BigUInt
        public var weightLimit: Substrate.WeightV2
        @StringCodable public var storageDepositLimit: BigUInt
        @BytesCodable public var data: Data

        enum CodingKeys: String, CodingKey {
            case dest
            case value
            case weightLimit = "weight_limit"
            case storageDepositLimit = "storage_deposit_limit"
            case data
        }

        public init(_ call: Call) {
            dest = call.dest
            value = call.value
            weightLimit = call.weightLimit
            storageDepositLimit = call.storageDepositLimit
            data = call.data
        }
    }

    /// `Revive.call` arguments on runtimes from before the rename, which still say `gas_limit`.
    public struct LegacyCallArgs: Codable {
        @BytesCodable public var dest: EvmAddress
        @StringCodable public var value: BigUInt
        public var gasLimit: Substrate.WeightV2
        @StringCodable public var storageDepositLimit: BigUInt
        @BytesCodable public var data: Data

        enum CodingKeys: String, CodingKey {
            case dest
            case value
            case gasLimit = "gas_limit"
            case storageDepositLimit = "storage_deposit_limit"
            case data
        }

        public init(_ call: Call) {
            dest = call.dest
            value = call.value
            gasLimit = call.weightLimit
            storageDepositLimit = call.storageDepositLimit
            data = call.data
        }
    }

    public static func weightLimitArgument(in metadata: any RuntimeMetadataProtocol) -> WeightLimitArgument {
        let arguments = metadata.getCall(from: name, with: callName)?.arguments ?? []
        return arguments.contains { $0.name == WeightLimitArgument.weightLimit.rawValue }
            ? .weightLimit
            : .gasLimit
    }
}
