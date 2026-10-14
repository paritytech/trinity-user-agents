import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    enum DeviceChange {
        case added(PeerDevice)
        case removed(statementAccountId: Data)
    }

    struct ContactDeviceSettings {
        let accountId: AccountId
        let changes: [DeviceChange]
    }
}

final class ContactDeviceSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.ContactDeviceSettings
    typealias CoreDataEntity = CDChatContact
}

extension ContactDeviceSettingsMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingContact
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingContact
        }

        let existingDevices = entity.devices as? Set<CDContactDevice> ?? []
        var devicesByAccountId = existingDevices.reduce(into: [Data: CDContactDevice]()) { result, device in
            if let statementAccountId = device.statementAccountId {
                result[statementAccountId] = device
            }
        }

        for change in model.changes {
            switch change {
            case let .added(device):
                let deviceEntity = try devicesByAccountId[device.statementAccountId]
                    ?? context.insertNew(CDContactDevice.self)
                deviceEntity.statementAccountId = device.statementAccountId
                deviceEntity.encryptionPublicKey = device.encryptionPublicKey
                deviceEntity.contact = entity
                devicesByAccountId[device.statementAccountId] = deviceEntity
                deviceEntity.touchParent()

            case let .removed(statementAccountId):
                guard let existing = devicesByAccountId.removeValue(forKey: statementAccountId) else {
                    continue
                }
                existing.touchParent()
                context.delete(existing)
            }
        }
    }
}

extension Chat.ContactDeviceSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}

extension CDContactDevice {
    func touchParent() {
        guard let parent = contact else { return }
        let key = #keyPath(CDChatContact.devices)
        parent.willChangeValue(forKey: key)
        parent.didChangeValue(forKey: key)
    }
}
