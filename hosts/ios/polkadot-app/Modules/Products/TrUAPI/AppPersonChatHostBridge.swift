import ChainRegistry
import Foundation
import Operation_iOS
import TrUAPIHost

/// The person chat services of products: the HOP nodes the native chat uploads to and claims from (the
/// `externalApi.hop` entries of the Bulletin chain in the chain registry), the devices paired with this wallet,
/// which the native chat announces to contacts by their statement account and SSO session key, and the contacts of the
/// chat product, which ``ProductChatContacts`` keeps for the rest of the app, and the native chat history
/// ``ProductChatMigration`` stages for the chat product.
final class AppPersonChatHostBridge: PersonChatHostBridge, @unchecked Sendable {
    private let nodes: @Sendable () -> [String]
    private let devices: @Sendable () async throws -> [HostPairedDevice]
    private let contacts: ProductChatContacts
    private let migration: ProductChatMigration

    init(
        nodes: @escaping @Sendable () -> [String],
        devices: @escaping @Sendable () async throws -> [HostPairedDevice],
        contacts: ProductChatContacts = ProductChatContacts(),
        migration: ProductChatMigration = ProductChatMigration()
    ) {
        self.nodes = nodes
        self.devices = devices
        self.contacts = contacts
        self.migration = migration
    }

    convenience init(
        chainRegistry: ChainRegistryProtocol,
        chainId: ChainModel.Id = AppConfig.Chains.bulletInChain,
        deviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        migration: ProductChatMigration
    ) {
        let repository = deviceRepositoryFactory.createRepository(forFilter: nil)
        self.init(
            nodes: {
                let apis = chainRegistry.getChain(for: chainId)?.externalApis?.hop() ?? []
                return apis.map(\.url.absoluteString).sorted()
            },
            devices: {
                try await repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute().map {
                    HostPairedDevice(
                        statementAccountId: $0.statementAccountId,
                        encryptionPublicKey: $0.encryptionPublicKey,
                        name: $0.hostName
                    )
                }
            },
            migration: migration
        )
    }

    func hopNodes() async throws -> [String] {
        nodes()
    }

    func pairedDevices() async throws -> [HostPairedDevice] {
        try await devices()
    }

    func setContacts(productId: String, contacts: [HostChatContact]) async throws {
        try self.contacts.replace(productId: productId, contacts: contacts)
    }

    func historyExport(productId: String, offset: UInt64, maxLen: UInt32) async throws -> HostHistoryExportChunk {
        try await migration.part(offset: offset, maxLen: maxLen)
    }

    func historyImported(productId: String) async throws {
        await migration.imported()
    }
}
