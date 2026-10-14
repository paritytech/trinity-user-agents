import Foundation
import CoreData
import Coinage
import Operation_iOS

/// Maps a `CDVoucher` to a ``TrackedVoucher`` — the raw voucher plus the durability overlay derived
/// from its input/output entry relations. Read-only: `populate` is unsupported.
final class TrackedVoucherMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = TrackedVoucher
    typealias CoreDataEntity = CDVoucher

    private let voucherMapper = VoucherMapper()
    private let currentInstallation: CoinageCurrentInstallationContextReader

    init(currentInstallation: CoinageCurrentInstallationContextReader = .init()) {
        self.currentInstallation = currentInstallation
    }
}

extension TrackedVoucherMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        let voucher = try voucherMapper.transform(entity: entity)
        return try TrackedVoucher(
            voucher: voucher,
            state: CoinageAssetStateDeriver.state(
                handedOff: false,
                isRecovered: currentInstallation.isRecovered(voucher.derivationIndex, of: entity),
                inputs: entity.coinageTxInputs,
                output: entity.coinageTxOutput
            )
        )
    }

    func populate(
        entity _: CoreDataEntity,
        from _: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        throw CoreDataMapperError.unsupported
    }
}
