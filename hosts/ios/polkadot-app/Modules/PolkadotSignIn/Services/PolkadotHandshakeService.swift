import Foundation
import FoundationExt
import Individuality
import MessageExchangeKit
import StatementStore
import NovaCrypto
import Operation_iOS
import SubstrateSdk
import AsyncExtensions
import KeyDerivation
import ChainRegistry

protocol PolkadotHandshakeServicing: AnyObject {
    func prepareInput(for url: URL) async throws -> HandshakeInput?
    func sendHandshake(with input: HandshakeInput) async throws -> Chat.LocalDevice
}

struct HandshakeInput {
    let hostData: HandshakeProposal
    let signerKeypair: SNKeypairProtocol
    let rootAccountId: Data
    let identityAccountId: Data
    let topic: Data
    let channel: Data
    let metadata: HandshakeMetadata
}

struct HandshakeMetadata {
    let name: String
    let iconUrl: URL?
}

final class PolkadotHandshakeService {
    private let rootWallet: WalletManaging
    private let identityWallet: WalletManaging
    private let sender: PolkadotHandshakeSending
    private let metadataFactory: PolkadotHandshakeMetadataMaking
    private let persistence: PolkadotHandshakePersisting
    private let logger: LoggerProtocol

    init(
        rootWallet: WalletManaging,
        identityWallet: WalletManaging,
        chatEncryptorFactory: MessageExchangeEncryptionMaking,
        ssoEncryptorFactory: MessageExchangeEncryptionMaking,
        sssManager: AllowanceManaging,
        deviceEncryptionKeyManager: DeviceEncryptionKeyManaging = DeviceEncryptionKeyManager.shared,
        rootEntropySourceDeriver: any RootEntropySourceDeriving = RootEntropySourceDeriver(
            entropyManager: RootEntropyManager.shared
        ),
        chainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        priorityProvider: HostHandshakePriorityProviding = HostHandshakePriorityProvider(),
        metadataFactory: PolkadotHandshakeMetadataMaking = PolkadotHandshakeMetadataFactory(),
        persistence: PolkadotHandshakePersisting = PolkadotHandshakePersistence(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.rootWallet = rootWallet
        self.identityWallet = identityWallet
        self.metadataFactory = metadataFactory
        self.persistence = persistence
        self.logger = logger

        sender = PolkadotHandshakeSender(
            payloadFactory: PolkadotHandshakePayloadFactory(
                chatEncryptorFactory: chatEncryptorFactory,
                ssoEncryptorFactory: ssoEncryptorFactory,
                deviceEncryptionKeyManager: deviceEncryptionKeyManager,
                rootEntropySourceDeriver: rootEntropySourceDeriver
            ),
            chainId: chainId,
            chainRegistry: chainRegistry,
            priorityProvider: priorityProvider,
            sssManager: sssManager,
            logger: logger
        )
    }
}

extension PolkadotHandshakeService: PolkadotHandshakeServicing {
    func prepareInput(for url: URL) async throws -> HandshakeInput? {
        guard let hostData = HandshakeProposal(url: url) else {
            return nil
        }
        return await makeHandshakeInput(hostData: hostData)
    }

    func sendHandshake(with input: HandshakeInput) async throws -> Chat.LocalDevice {
        try await sender.sendResponse(for: input)
        return try await persistLocally(input: input)
    }
}

private extension PolkadotHandshakeService {
    // MARK: - Input Assembly

    func makeHandshakeInput(hostData: HandshakeProposal) async -> HandshakeInput? {
        do {
            let deviceData = hostData.deviceData
            let rootAccountId = try rootWallet.getRawPublicKey()
            let identityAccountId = try identityWallet.getRawPublicKey()
            let signerKeypair = try makeStatementSignerKeypair()

            let topic = try makeTopic(deviceData: deviceData)
            let channel = try makeChannel(deviceData: deviceData)

            let metadata = try await metadataFactory.makeMetadata(from: hostData)

            return HandshakeInput(
                hostData: hostData,
                signerKeypair: signerKeypair,
                rootAccountId: rootAccountId,
                identityAccountId: identityAccountId,
                topic: topic,
                channel: channel,
                metadata: metadata
            )
        } catch {
            logger.error("Failed to make handshake input: \(error)")
            return nil
        }
    }

    // MARK: - Persistence

    func persistLocally(
        input: HandshakeInput
    ) async throws -> Chat.LocalDevice {
        let deviceData = input.hostData.deviceData

        try await persistence.saveHostData(
            deviceData: deviceData,
            metadata: input.metadata
        )
        logger.debug("Saved polkadot host: \(input.metadata.name)")

        let device = try await persistence.saveDeviceData(
            from: deviceData,
            hostName: input.metadata.name
        )
        logger.debug("Saved local device: \(deviceData.statementAccountId.toHex())")

        return device
    }

    // MARK: - Helpers

    func makeTopic(deviceData: HandshakeDeviceData) throws -> Data {
        let message = deviceData.encryptionPublicKey + Data("topic".utf8)
        let secret = deviceData.statementAccountId
        return try message.blake2b32WithKey(secret)
    }

    func makeChannel(deviceData: HandshakeDeviceData) throws -> Data {
        let message = deviceData.encryptionPublicKey + Data("channel".utf8)
        let secret = deviceData.statementAccountId
        return try message.blake2b32WithKey(secret)
    }

    func makeStatementSignerKeypair() throws -> SNKeypair {
        // TODO: Derive shared statement store keypair when it's implemented on the pallet side
        let rawPublicKey = try identityWallet.getRawPublicKey()
        let publicKey = try SNPublicKey(rawData: rawPublicKey)
        let rawSecretKey = try identityWallet.fetchSignerSecret(for: publicKey)
        let secretKey = try SNPrivateKey(rawData: rawSecretKey)
        return SNKeypair(privateKey: secretKey, publicKey: publicKey)
    }
}
