import Foundation
import MessageExchangeKit

enum MultideviceComponentFactory {
    static func makeDeviceUpdateProcessor(
        contactsStorageService: ContactsLocalStorageServicing,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) -> DeviceUpdateProcessor {
        DeviceUpdateProcessor(
            contactsStorageService: contactsStorageService,
            messageExchangeModeProvider: messageExchangeModeProvider,
            workQueue: workQueue,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    static func makeSenderDeviceActivator(
        contactsStorageService: ContactsLocalStorageServicing,
        chatRequestStoreService: ChatRequestStoreServicing,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        logger: LoggerProtocol = Logger.shared
    ) -> SenderDeviceActivator {
        SenderDeviceActivator(
            contactsStorageService: contactsStorageService,
            chatRequestStoreService: chatRequestStoreService,
            messageExchangeModeProvider: messageExchangeModeProvider,
            logger: logger
        )
    }

    static func makeDeviceMessageBroadcaster(
        messageExchangeModeProvider: MessageExchangeModeProviding,
        logger: LoggerProtocol = Logger.shared
    ) -> DeviceMessageBroadcaster {
        DeviceMessageBroadcaster(
            messageExchangeModeProvider: messageExchangeModeProvider,
            logger: logger
        )
    }

    static func makePendingDeviceFanOutProcessor(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        logger: LoggerProtocol = Logger.shared
    ) -> PendingDeviceFanOutProcessor {
        PendingDeviceFanOutProcessor(
            chatContactDataProviderFactory: chatContactDataProviderFactory,
            messageExchangeModeProvider: messageExchangeModeProvider,
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
