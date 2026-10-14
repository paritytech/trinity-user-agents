import Foundation
import Operation_iOS
import CoreData
import NovaCrypto
import SubstrateSdk
import SDKLogger

protocol ContactsLocalStorageServicing {
    func getContact(
        by accountId: AccountId
    ) -> BaseOperation<Chat.Contact?>

    func getContact(
        byPushId pushId: String
    ) -> CompoundOperationWrapper<Chat.Contact?>

    func updateOugoingSettings(_ settingsList: [Chat.ContactOutgoingSettings]) -> BaseOperation<Void>
    func updateVoIPOugoingSettings(_ settingsList: [Chat.VoIPContactOutgoingSettings]) -> BaseOperation<Void>

    func updateIncomingSettings(_ settingsList: [Chat.ContactIncomingSettings]) -> BaseOperation<Void>
    func updateVoIPIncomingSettings(_ settingsList: [Chat.VoIPContactIncomingSettings]) -> BaseOperation<Void>
    func updateDeviceSettings(_ settingsList: [Chat.ContactDeviceSettings]) -> BaseOperation<Void>
}

final class ContactsLocalStorageService: ContactsLocalStorageServicing {
    private let storageFacade: StorageFacadeProtocol
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.logger = logger
    }

    func getContact(
        by accountId: AccountId
    ) -> BaseOperation<Chat.Contact?> {
        let repository = storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ChatContactMapper())
        )

        return repository.fetchOperation(by: { accountId.toHex() }, options: .init())
    }

    func getContact(
        byPushId pushId: String
    ) -> CompoundOperationWrapper<Chat.Contact?> {
        let key = #keyPath(CDChatContact.pushId)
        let filter = NSPredicate(format: "\(key) == %@", pushId)
        let repository = storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatContactMapper())
        )

        let fetchOperation = repository.fetchAllOperation(with: .init())

        let mappingOperation = ClosureOperation {
            try fetchOperation.extractNoCancellableResultData().first
        }
        mappingOperation.addDependency(fetchOperation)

        return CompoundOperationWrapper(
            targetOperation: mappingOperation,
            dependencies: [fetchOperation]
        )
    }

    func updateOugoingSettings(_ settingsList: [Chat.ContactOutgoingSettings]) -> BaseOperation<Void> {
        storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ContactOutgoingSettingsMapper())
        )
        .saveOperation({ settingsList }, { [] })
    }

    func updateVoIPOugoingSettings(_ settingsList: [Chat.VoIPContactOutgoingSettings]) -> BaseOperation<Void> {
        storageFacade.createRepository(
            mapper: AnyCoreDataMapper(VoIPContactOutgoingSettingsMapper())
        )
        .saveOperation({ settingsList }, { [] })
    }

    func updateIncomingSettings(_ settingsList: [Chat.ContactIncomingSettings]) -> BaseOperation<Void> {
        storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ContactIncomingSettingsMapper())
        )
        .saveOperation({ settingsList }, { [] })
    }

    func updateVoIPIncomingSettings(_ settingsList: [Chat.VoIPContactIncomingSettings]) -> BaseOperation<Void> {
        storageFacade.createRepository(
            mapper: AnyCoreDataMapper(VoIPContactIncomingSettingsMapper())
        )
        .saveOperation({ settingsList }, { [] })
    }

    func updateDeviceSettings(_ settingsList: [Chat.ContactDeviceSettings]) -> BaseOperation<Void> {
        storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ContactDeviceSettingsMapper())
        )
        .saveOperation({ settingsList }, { [] })
    }
}
