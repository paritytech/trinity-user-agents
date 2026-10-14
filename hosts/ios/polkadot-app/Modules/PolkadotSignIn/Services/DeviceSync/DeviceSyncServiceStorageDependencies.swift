struct DeviceSyncServiceStorageDependencies {
    let deviceDataProviderFactory: LocalDeviceDataProviderMaking
    let deviceRepositoryFactory: LocalDeviceRepositoryMaking
    let contactRepositoryFactory: ChatContactRepositoryMaking
    let chatRepositoryFactory: ChatRepositoryMaking
    let messageRepositoryFactory: ChatMessageRepositoryMaking
    let removedChatRepositoryFactory: RemovedChatRepositoryMaking
    let lastSyncOfferIdRepositoryFactory: LastSyncOfferIdRepositoryMaking
    let outgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking

    init(
        deviceDataProviderFactory: LocalDeviceDataProviderMaking = LocalDeviceDataProviderFactory(),
        deviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        contactRepositoryFactory: ChatContactRepositoryMaking = ChatContactRepositoryFactory(),
        chatRepositoryFactory: ChatRepositoryMaking = ChatRepositoryFactory(),
        messageRepositoryFactory: ChatMessageRepositoryMaking = ChatMessageRepositoryFactory(),
        removedChatRepositoryFactory: RemovedChatRepositoryMaking = RemovedChatRepositoryFactory(),
        lastSyncOfferIdRepositoryFactory: LastSyncOfferIdRepositoryMaking = LastSyncOfferIdRepositoryFactory(),
        outgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking =
            OutgoingUpdateTimeRepositoryFactory()
    ) {
        self.deviceDataProviderFactory = deviceDataProviderFactory
        self.deviceRepositoryFactory = deviceRepositoryFactory
        self.contactRepositoryFactory = contactRepositoryFactory
        self.chatRepositoryFactory = chatRepositoryFactory
        self.messageRepositoryFactory = messageRepositoryFactory
        self.removedChatRepositoryFactory = removedChatRepositoryFactory
        self.lastSyncOfferIdRepositoryFactory = lastSyncOfferIdRepositoryFactory
        self.outgoingUpdateTimeRepositoryFactory = outgoingUpdateTimeRepositoryFactory
    }
}
