import Foundation
import MessageExchangeKit

enum MultideviceComponentFactory {
    static func makeDeviceUpdateProcessor(
        contactsStorageService: ContactsLocalStorageServicing,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) -> DeviceUpdateProcessor {
        DeviceUpdateProcessor(
            contactsStorageService: contactsStorageService,
            workQueue: workQueue,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    static func makeDeviceMessageBroadcaster(logger: LoggerProtocol = Logger.shared) -> DeviceMessageBroadcaster {
        DeviceMessageBroadcaster(logger: logger)
    }

    static func makePendingDeviceFanOutProcessor(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        logger: LoggerProtocol = Logger.shared
    ) -> PendingDeviceFanOutProcessor {
        PendingDeviceFanOutProcessor(
            chatContactDataProviderFactory: chatContactDataProviderFactory,
            logger: logger
        )
    }

    static func makeDeviceEncryptionKeyFactory(
        deviceEncryptionKeyManager: DeviceEncryptionKeyManaging
    ) throws -> MessageExchangeEncryptionMaking {
        let devicePrivateKey = try deviceEncryptionKeyManager.getOrCreatePrivateKey()
        return X25519ChaChaPolyEncryptorFactory(privateKey: devicePrivateKey)
    }
}
