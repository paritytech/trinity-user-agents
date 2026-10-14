import Foundation
import BigInt
import SubstrateSdk

public struct XcmTransfers {
    let legacyTransfers: XcmLegacyTransfers
    let dynamicTransfers: XcmDynamicTransfers
    let generalConfig: XcmGeneralConfig

    let indexedByOrigins: [ChainAssetId: Set<ChainAssetId>]
    let indexedByDestinations: [ChainAssetId: Set<ChainAssetId>]

    public init(
        legacyTransfers: XcmLegacyTransfers,
        dynamicTransfers: XcmDynamicTransfers,
        generalConfig: XcmGeneralConfig
    ) {
        self.legacyTransfers = legacyTransfers
        self.dynamicTransfers = dynamicTransfers
        self.generalConfig = generalConfig

        var indexedByOrigins: [ChainAssetId: Set<ChainAssetId>] = [:]
        var indexedByDestinations: [ChainAssetId: Set<ChainAssetId>] = [:]

        let allChains: [XcmTransferChainProtocol] = legacyTransfers.getChains() + dynamicTransfers.getChains()

        for chain in allChains {
            for asset in chain.getAssets() {
                let origin = ChainAssetId(chainId: chain.chainId, assetId: asset.assetId)
                for transfer in asset.getDestinations() {
                    let destination = ChainAssetId(chainId: transfer.chainId, assetId: transfer.assetId)

                    if indexedByOrigins[origin] == nil {
                        indexedByOrigins[origin] = [destination]
                    } else {
                        indexedByOrigins[origin]?.insert(destination)
                    }

                    if indexedByDestinations[destination] == nil {
                        indexedByDestinations[destination] = [origin]
                    } else {
                        indexedByDestinations[destination]?.insert(origin)
                    }
                }
            }
        }

        self.indexedByOrigins = indexedByOrigins
        self.indexedByDestinations = indexedByDestinations
    }
}

public enum XcmTransfersError: Error {
    case noTransfer(ChainAssetId, ChainId)
    case noReserve(ChainAssetId)
    case noInstructions(String)
    case deliveryFeeNotAvailable
    case noDestinationFee(origin: ChainAssetId, destination: ChainId)
    case noBaseWeight(ChainId)
}

private extension XcmTransfers {
    func checkLegacyDeliveryFee(
        from originChain: ChainProtocol,
        destinationChain: ChainProtocol
    ) -> Bool {
        do {
            let deliveryFee = try legacyTransfers.deliveryFee(from: originChain.chainId)

            if !destinationChain.isRelaychain {
                return deliveryFee?.toParachain?.alwaysHoldingPays ?? false
            } else if !originChain.isRelaychain {
                return deliveryFee?.toParent?.alwaysHoldingPays ?? false
            } else {
                return false
            }
        } catch {
            return true
        }
    }

    func getLegacyDestinationFeeParams(
        for chainAsset: ChainAssetProtocol,
        destinationChain: ChainProtocol
    ) throws -> XcmTransferMetadata.LegacyFeeDetails {
        guard let destinationFee = legacyTransfers.transfer(
            from: chainAsset.chainAssetId,
            destinationChainId: destinationChain.chainId
        )?.destination.fee else {
            throw XcmTransfersError.noTransfer(
                chainAsset.chainAssetId,
                destinationChain.chainId
            )
        }

        guard
            let destinationInstructions = legacyTransfers.instructions(
                for: destinationFee.instructions
            ) else {
            throw XcmTransfersError.noInstructions(destinationFee.instructions)
        }

        guard let destinationBaseWeight = legacyTransfers.baseWeight(for: destinationChain.chainId) else {
            throw XcmTransfersError.noBaseWeight(destinationChain.chainId)
        }

        return XcmTransferMetadata.LegacyFeeDetails(
            instructions: destinationInstructions,
            mode: destinationFee,
            baseWeight: destinationBaseWeight
        )
    }

    func getLegacyReserveFeeParams(
        for originChainAsset: ChainAssetProtocol
    ) throws -> XcmTransferMetadata.LegacyFeeDetails? {
        guard let reserveFee = legacyTransfers.reserveFee(from: originChainAsset.chainAssetId) else {
            return nil
        }

        guard let reserveFeeInstructions = legacyTransfers.instructions(
            for: reserveFee.instructions
        ) else {
            throw XcmTransfersError.noInstructions(reserveFee.instructions)
        }

        guard let reserveId = legacyTransfers.getReserveChainId(for: originChainAsset.chainAssetId) else {
            throw XcmTransfersError.noReserve(originChainAsset.chainAssetId)
        }

        guard let reserveBaseWeight = legacyTransfers.baseWeight(for: reserveId) else {
            throw XcmTransfersError.noBaseWeight(reserveId)
        }

        return XcmTransferMetadata.LegacyFeeDetails(
            instructions: reserveFeeInstructions,
            mode: reserveFee,
            baseWeight: reserveBaseWeight
        )
    }

    func getLegacyFeeParams(
        for chainAsset: ChainAssetProtocol,
        destinationChain: ChainProtocol
    ) throws -> XcmTransferMetadata.LegacyFee {
        let destinationDetails = try getLegacyDestinationFeeParams(
            for: chainAsset,
            destinationChain: destinationChain
        )

        let reserveDetails = try getLegacyReserveFeeParams(for: chainAsset)
        let originDelivery = try legacyTransfers.deliveryFee(from: chainAsset.chainInterface.chainId)

        guard let reserveId = legacyTransfers.getReserveChainId(for: chainAsset.chainAssetId) else {
            throw XcmTransfersError.noReserve(chainAsset.chainAssetId)
        }

        let reserveDeliveryFee = try legacyTransfers.deliveryFee(from: reserveId)

        return XcmTransferMetadata.LegacyFee(
            destinationExecution: destinationDetails,
            reserveExecution: reserveDetails,
            originDelivery: originDelivery,
            reserveDelivery: reserveDeliveryFee
        )
    }

    func getLegacyTransferMetadata(
        for chainAsset: ChainAssetProtocol,
        destinationChain: ChainProtocol
    ) throws -> XcmTransferMetadata? {
        guard
            let transfer = legacyTransfers.transfer(
                from: chainAsset.chainAssetId,
                destinationChainId: destinationChain.chainId
            ) else {
            return nil
        }

        guard
            let reservePath = legacyTransfers.getReservePath(for: chainAsset.chainAssetId),
            let reserveId = legacyTransfers.getReserveChainId(for: chainAsset.chainAssetId) else {
            throw XcmTransfersError.noReserve(chainAsset.chainAssetId)
        }

        let paysDeliveryFee = checkLegacyDeliveryFee(
            from: chainAsset.chainInterface,
            destinationChain: destinationChain
        )

        let feeParams = try getLegacyFeeParams(
            for: chainAsset,
            destinationChain: destinationChain
        )

        return XcmTransferMetadata(
            callType: transfer.type,
            reserve: XcmTransferMetadata.Reserve(
                reserveId: reserveId,
                path: reservePath
            ),
            fee: .legacy(feeParams),
            paysDeliveryFee: paysDeliveryFee,
            supportsXcmExecute: false,
            usesTeleport: false
        )
    }

    func getDynamicTransferMetadata(
        for chainAsset: ChainAssetProtocol,
        destinationChain: ChainProtocol
    ) throws -> XcmTransferMetadata? {
        guard
            let transfer = dynamicTransfers.transfer(
                from: chainAsset.chainAssetId,
                destinationChainId: destinationChain.chainId
            ) else {
            return nil
        }

        guard
            let reservePath = generalConfig.assets.getReservePath(for: chainAsset),
            let reserveId = generalConfig.assets.getReserveChainId(for: chainAsset) else {
            throw XcmTransfersError.noReserve(chainAsset.chainAssetId)
        }

        let usesTeleport = dynamicTransfers.getUsesCustomTeleport(
            from: chainAsset.chainAssetId,
            destination: destinationChain.chainId
        )

        return XcmTransferMetadata(
            callType: transfer.type,
            reserve: XcmTransferMetadata.Reserve(
                reserveId: reserveId,
                path: reservePath
            ),
            fee: .dynamic,
            paysDeliveryFee: transfer.hasDeliveryFee ?? false,
            supportsXcmExecute: transfer.supportsXcmExecute ?? false,
            usesTeleport: usesTeleport
        )
    }
}

public extension XcmTransfers {
    func getTransferMetadata(
        for chainAsset: ChainAssetProtocol,
        destinationChain: ChainProtocol
    ) throws -> XcmTransferMetadata {
        if let dynamicMetadata = try getDynamicTransferMetadata(
            for: chainAsset,
            destinationChain: destinationChain
        ) {
            return dynamicMetadata
        }

        if let legacyMetadata = try getLegacyTransferMetadata(
            for: chainAsset,
            destinationChain: destinationChain
        ) {
            return legacyMetadata
        }

        throw XcmTransfersError.noTransfer(chainAsset.chainAssetId, destinationChain.chainId)
    }

    func getAllTransfers() -> [ChainAssetId: Set<ChainAssetId>] {
        indexedByOrigins
    }

    func getOrigins(for chainAssetId: ChainAssetId) -> Set<ChainAssetId> {
        indexedByDestinations[chainAssetId] ?? []
    }

    func getDestinations(for chainAssetId: ChainAssetId) -> Set<ChainAssetId> {
        indexedByOrigins[chainAssetId] ?? []
    }
}

public struct XcmTransferMetadata {
    public struct Reserve {
        public let reserveId: ChainId
        public let path: XcmAsset.ReservePath

        public init(reserveId: ChainId, path: XcmAsset.ReservePath) {
            self.reserveId = reserveId
            self.path = path
        }
    }

    public enum Fee {
        case legacy(LegacyFee)
        case dynamic
    }

    public struct LegacyFeeDetails {
        public let instructions: [String]
        public let mode: XcmAssetTransferFee
        public let baseWeight: BigUInt

        public var maxWeight: BigUInt {
            baseWeight * BigUInt(instructions.count)
        }

        public init(instructions: [String], mode: XcmAssetTransferFee, baseWeight: BigUInt) {
            self.instructions = instructions
            self.mode = mode
            self.baseWeight = baseWeight
        }
    }

    public struct LegacyFee {
        public let destinationExecution: LegacyFeeDetails
        public let reserveExecution: LegacyFeeDetails?
        public let originDelivery: XcmDeliveryFee?
        public let reserveDelivery: XcmDeliveryFee?

        public var maxWeight: BigUInt {
            let reserveMaxWeight = reserveExecution?.maxWeight ?? 0

            return destinationExecution.maxWeight + reserveMaxWeight
        }

        public init(
            destinationExecution: LegacyFeeDetails,
            reserveExecution: LegacyFeeDetails?,
            originDelivery: XcmDeliveryFee?,
            reserveDelivery: XcmDeliveryFee?
        ) {
            self.destinationExecution = destinationExecution
            self.reserveExecution = reserveExecution
            self.originDelivery = originDelivery
            self.reserveDelivery = reserveDelivery
        }
    }

    public let callType: XcmCallType
    public let reserve: Reserve
    public let fee: Fee
    public let paysDeliveryFee: Bool
    public let supportsXcmExecute: Bool
    public let usesTeleport: Bool

    public var isDynamicConfig: Bool {
        switch fee {
        case .dynamic:
            true
        default:
            false
        }
    }

    public init(
        callType: XcmCallType,
        reserve: Reserve,
        fee: Fee,
        paysDeliveryFee: Bool,
        supportsXcmExecute: Bool,
        usesTeleport: Bool
    ) {
        self.callType = callType
        self.reserve = reserve
        self.fee = fee
        self.paysDeliveryFee = paysDeliveryFee
        self.supportsXcmExecute = supportsXcmExecute
        self.usesTeleport = usesTeleport
    }
}
