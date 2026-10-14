import Foundation
import CoreData
import Coinage
import Operation_iOS
import SubstrateSdk

/// Maps a `CDCoin` to the raw ``Coin`` — every stored field, no derived status. The durability
/// overlay is added separately by ``TrackedCoinMapper``.
final class CoinMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Coin
    typealias CoreDataEntity = CDCoin
}

extension CoinMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let handoffMark = CoinHandoffMark(rawValue: entity.handoffMark) else {
            throw CoreDataMapperError.unexpected(#keyPath(CDCoin.handoffMark))
        }

        guard let publicKeyHex = entity.publicKey else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoin.publicKey))
        }

        return try Coin(
            exponent: entity.exponent,
            derivationIndex: entity.keyIndex(),
            age: entity.age?.int16Value,
            isOnchain: entity.isOnchain,
            handoffMark: handoffMark,
            recyclerFungibility: entity.recyclerFungibility.map { UInt8(clamping: $0.intValue) },
            hops: Self.hops(from: entity.hops),
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
        entity.age = model.age.map { NSNumber(value: $0) }
        entity.isOnchain = model.isOnchain
        entity.handoffMark = model.handoffMark.rawValue
        entity.recyclerFungibility = model.recyclerFungibility.map { NSNumber(value: $0) }
        entity.hops = try Self.encoded(hops: model.hops)
        entity.publicKey = model.publicKey.toHex()
    }
}

private extension CoinMapper {
    static func hops(from data: Data?) throws -> [Hop] {
        guard let data, !data.isEmpty else { return [] }

        let decoder = try ScaleDecoder(data: data)
        return try [CodableHop](scaleDecoder: decoder).map(\.hop)
    }

    static func encoded(hops: [Hop]) throws -> Data? {
        guard !hops.isEmpty else { return nil }

        let encoder = ScaleEncoder()
        try hops.map(CodableHop.init).encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}

/// SCALE representation of ``Hop``.
///
/// Tags are a persisted format: append new cases with new numbers, never renumber or reuse
/// an existing tag.
private struct CodableHop: ScaleCodable {
    let hop: Hop

    init(_ hop: Hop) {
        self.hop = hop
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let tag = try UInt8(scaleDecoder: scaleDecoder)

        switch tag {
        case 0:
            hop = try .transfer(bundleSize: UInt8(scaleDecoder: scaleDecoder))
        case 1:
            hop = try .split(fanout: UInt8(scaleDecoder: scaleDecoder))
        default:
            throw CoinMapperError.unknownHopTag(tag)
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch hop {
        case let .transfer(bundleSize):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try bundleSize.encode(scaleEncoder: scaleEncoder)
        case let .split(fanout):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try fanout.encode(scaleEncoder: scaleEncoder)
        }
    }
}

private enum CoinMapperError: Error {
    case unknownHopTag(UInt8)
}
