import CoreData
import Operation_iOS
import Products

/// A card's row: what the user added, and the newest face its product drew.
///
/// Both halves live in one row, and either can be absent. A host-placed card is
/// placed on every run rather than stored, so its row carries only a face;
/// a card added before its product has drawn carries only membership.
struct StoredPocketCard: Identifiable, Equatable {
    /// What the user added. Absent on a row that exists only to keep the face a
    /// host-placed card was last drawn with.
    struct Membership: Equatable {
        let title: String
        let addedAt: Date
    }

    let key: PocketCardKey
    let membership: Membership?
    let face: Data?

    var identifier: String { key.storageId }
    var id: String { identifier }
}

/// Writes the membership half of a row and reads the whole of it.
///
/// Membership and the face are written through separate mappers so neither
/// write has to read the row back first, which is what would let a face landing
/// at frame rate and a card being added overwrite each other.
final class PocketCardMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = StoredPocketCard
    typealias CoreDataEntity = CDPocketCard
}

extension PocketCardMapper: CoreDataMapperProtocol {
    func transform(entity: CDPocketCard) throws -> StoredPocketCard {
        guard let productId = entity.productId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDPocketCard.productId))
        }

        guard let cardId = entity.cardId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDPocketCard.cardId))
        }

        var membership: StoredPocketCard.Membership?
        if let title = entity.title, let addedAt = entity.addedAt {
            membership = StoredPocketCard.Membership(title: title, addedAt: addedAt)
        }

        return StoredPocketCard(
            key: PocketCardKey(productId: productId, cardId: PocketCardId(value: cardId)),
            membership: membership,
            face: entity.face
        )
    }

    func populate(entity: CDPocketCard, from model: StoredPocketCard, using _: NSManagedObjectContext) throws {
        entity.identifier = model.identifier
        entity.productId = model.key.productId
        entity.cardId = model.key.cardId.value
        entity.title = model.membership?.title
        entity.addedAt = model.membership?.addedAt
    }
}
