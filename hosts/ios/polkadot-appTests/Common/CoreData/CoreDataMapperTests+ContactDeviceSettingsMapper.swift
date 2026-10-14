import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("ContactDeviceSettingsMapper")
    struct ContactDeviceSettingsMapperTests {
        private let facade = UserDataStorageTestFacade()
        private let contactAccountId = Data(repeating: 0x01, count: 32)

        @Test("duplicate adds keep one device and update key")
        func duplicateAddsKeepOneDeviceAndUpdateKey() async throws {
            try await seedContact(devices: [])

            let device = makeDevice(accountByte: 0x02, keyByte: 0x03)
            let updatedDevice = makeDevice(accountByte: 0x02, keyByte: 0x04)

            try await updateDevices([
                .added(device),
                .added(updatedDevice)
            ])

            let devices = try await fetchDevices()
            #expect(devices.count == 1)
            #expect(devices.first?.statementAccountId == updatedDevice.statementAccountId)
            #expect(devices.first?.encryptionPublicKey == updatedDevice.encryptionPublicKey)
        }

        @Test("add then remove leaves no device")
        func addThenRemoveLeavesNoDevice() async throws {
            try await seedContact(devices: [])

            let device = makeDevice(accountByte: 0x02, keyByte: 0x03)

            try await updateDevices([
                .added(device),
                .removed(statementAccountId: device.statementAccountId)
            ])

            let devices = try await fetchDevices()
            #expect(devices.isEmpty)
        }

        @Test("remove then add existing device replaces key")
        func removeThenAddExistingDeviceReplacesKey() async throws {
            let oldDevice = makeDevice(accountByte: 0x02, keyByte: 0x03)
            let newDevice = makeDevice(accountByte: 0x02, keyByte: 0x04)
            try await seedContact(devices: [oldDevice])

            try await updateDevices([
                .removed(statementAccountId: oldDevice.statementAccountId),
                .added(newDevice)
            ])

            let devices = try await fetchDevices()
            #expect(devices.count == 1)
            #expect(devices.first?.statementAccountId == newDevice.statementAccountId)
            #expect(devices.first?.encryptionPublicKey == newDevice.encryptionPublicKey)
        }
    }
}

private extension CoreDataMapperTests.ContactDeviceSettingsMapperTests {
    var contactRepository: AnyDataProviderRepository<Chat.Contact> {
        facade.makeRepo(mapper: ChatContactMapper())
    }

    var deviceSettingsRepository: AnyDataProviderRepository<Chat.ContactDeviceSettings> {
        facade.makeRepo(mapper: ContactDeviceSettingsMapper())
    }

    func seedContact(devices: [Chat.PeerDevice]) async throws {
        let contact = Chat.Contact(
            accountId: contactAccountId,
            username: "Alice",
            publicKey: Data(repeating: 0x05, count: 32),
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: "sign-key", encryptionKeyId: "encryption-key"),
            imageData: nil,
            isBlocked: false,
            devices: devices,
            pendingDevicesFanOut: false
        )

        try await contactRepository.saveOperation({ [contact] }, { [] }).asyncExecute()
    }

    func updateDevices(_ changes: [Chat.DeviceChange]) async throws {
        let settings = Chat.ContactDeviceSettings(
            accountId: contactAccountId,
            changes: changes
        )

        try await deviceSettingsRepository.saveOperation({ [settings] }, { [] }).asyncExecute()
    }

    func fetchDevices() async throws -> [Chat.PeerDevice] {
        let contact = try await contactRepository
            .fetchOperation(by: { contactAccountId.toHex() }, options: .init())
            .asyncExecute()

        return contact?.devices ?? []
    }

    func makeDevice(accountByte: UInt8, keyByte: UInt8) -> Chat.PeerDevice {
        Chat.PeerDevice(
            statementAccountId: Data(repeating: accountByte, count: 32),
            encryptionPublicKey: Data(repeating: keyByte, count: 32)
        )
    }
}
