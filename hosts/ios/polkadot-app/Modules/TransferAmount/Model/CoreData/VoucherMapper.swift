import Foundation
import CoreData
import Coinage
import Operation_iOS
import SubstrateSdk

final class VoucherMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Voucher
    typealias CoreDataEntity = CDVoucher
}

extension VoucherMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let allocatedAt = entity.allocatedAt else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.allocatedAt)
            )
        }

        guard let readyAt = entity.readyAt else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.readyAt)
            )
        }

        let recycler: Voucher.Recycler? =
            if entity.recyclerIndex >= 0 {
                Voucher.Recycler(
                    index: UInt32(entity.recyclerIndex),
                    membersCount: UInt32(max(0, entity.recyclerMembers)),
                    enteredAt: entity.enteredAt
                )
            } else {
                nil
            }

        let state: Voucher.OnChainState =
            if entity.onChainState == 1 {
                .onboarding
            } else if entity.onChainState == 2, let recycler {
                .inRecycler(recycler)
            } else {
                .unlocated
            }

        guard let publicKeyHex = entity.publicKey else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDVoucher.publicKey))
        }

        return try Voucher(
            exponent: entity.exponent,
            derivationIndex: entity.keyIndex(),
            allocatedAt: allocatedAt,
            readyAt: readyAt,
            remoteState: state,
            recyclerFungibility: Self.fungibility(from: entity.recyclerFungibility),
            maxRecyclerFungibility: Self.fungibility(from: entity.maxRecyclerFungibility),
            publicKey: Data(hexString: publicKeyHex)
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.installationId = model.derivationIndex.installation.hex
        entity.derivationIndex = Int64(bitPattern: model.derivationIndex.item)
        entity.exponent = model.exponent
        entity.readyAt = model.readyAt
        entity.allocatedAt = model.allocatedAt
        entity.recyclerIndex = model.recycler.flatMap { Int64($0.index) } ?? -1
        entity.recyclerMembers = model.recycler.map { Int64($0.membersCount) } ?? 0
        entity.enteredAt = model.recycler?.enteredAt
        entity.publicKey = model.publicKey.toHex()
        entity.recyclerFungibility = Int16(model.recyclerFungibility)
        entity.maxRecyclerFungibility = Int16(model.maxRecyclerFungibility)

        entity.onChainState =
            switch model.remoteState {
            case .unlocated: 0
            case .onboarding: 1
            case .inRecycler: 2
            }
    }
}

private extension VoucherMapper {
    /// Clamped so an out-of-range row can never trap on `UInt8` conversion.
    static func fungibility(from stored: Int16) -> UInt8 {
        UInt8(clamping: max(0, min(Int(CoinageConstants.fullFungibility), Int(stored))))
    }
}
