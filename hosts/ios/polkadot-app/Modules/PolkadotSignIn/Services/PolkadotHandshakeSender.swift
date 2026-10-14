import Foundation
import Individuality
import MessageExchangeKit
import StatementStore
import NovaCrypto
import ChainRegistry
import StructuredConcurrency

protocol PolkadotHandshakeSending {
    func sendResponse(for input: HandshakeInput) async throws
}

final class PolkadotHandshakeSender {
    private let payloadFactory: PolkadotHandshakePayloadMaking
    private let chainId: ChainModel.Id
    private let chainRegistry: ChainRegistryProtocol
    private let priorityProvider: HostHandshakePriorityProviding
    private let sssManager: AllowanceManaging
    private let logger: LoggerProtocol

    init(
        payloadFactory: PolkadotHandshakePayloadMaking,
        chainId: ChainModel.Id,
        chainRegistry: ChainRegistryProtocol,
        priorityProvider: HostHandshakePriorityProviding,
        sssManager: AllowanceManaging,
        logger: LoggerProtocol
    ) {
        self.payloadFactory = payloadFactory
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.priorityProvider = priorityProvider
        self.sssManager = sssManager
        self.logger = logger
    }
}

extension PolkadotHandshakeSender: PolkadotHandshakeSending {
    func sendResponse(for input: HandshakeInput) async throws {
        try await markStallActivity("SSO handshake") {
            let deviceData = input.hostData.deviceData

            switch input.hostData {
            case .v1:
                try await sendV1(with: input, deviceData: deviceData)
            case .v2:
                try await sendV2(with: input, deviceData: deviceData)
            }
        }
    }
}

private extension PolkadotHandshakeSender {
    // MARK: - V1 Flow

    func sendV1(
        with input: HandshakeInput,
        deviceData: HandshakeDeviceData
    ) async throws {
        logger.debug("Starting V1 flow...")

        do {
            try await allocateAllowance(for: input)
            try await markStallRegion("Sending statement") {
                try await submitStatement(
                    payload: payloadFactory.makeSuccessPayload(
                        hostData: input.hostData,
                        deviceData: deviceData,
                        rootAccountId: input.rootAccountId,
                        identityAccountId: input.identityAccountId
                    ),
                    input: input
                )
            }
            logger.debug("V1 flow completed successfully")
        } catch {
            logger.error("V1 flow failed: \(error)")
            throw error
        }
    }

    // MARK: - V2 Flow

    func sendV2(
        with input: HandshakeInput,
        deviceData: HandshakeDeviceData
    ) async throws {
        logger.debug("Starting V2 flow...")

        do {
            await sendStatusPayload(
                .pending(.allowanceAllocation),
                deviceData: deviceData,
                input: input
            )
            try await allocateAllowance(for: input)
            try await markStallRegion("Sending statement") {
                try await submitStatement(
                    payload: payloadFactory.makeSuccessPayload(
                        hostData: input.hostData,
                        deviceData: deviceData,
                        rootAccountId: input.rootAccountId,
                        identityAccountId: input.identityAccountId
                    ),
                    input: input
                )
            }
            logger.debug("V2 flow completed successfully")
        } catch {
            logger.error("V2 flow failed: \(error)")
            await sendStatusPayload(
                .failed(error.localizedDescription),
                deviceData: deviceData,
                input: input
            )
            throw error
        }
    }

    func sendStatusPayload(
        _ response: EncryptedHandshakeResponseV2,
        deviceData: HandshakeDeviceData,
        input: HandshakeInput
    ) async {
        do {
            try await submitStatement(
                payload: payloadFactory.makeV2StatusPayload(
                    response: response,
                    deviceData: deviceData
                ),
                input: input
            )
            logger.debug("Sent \(response) status to host")
        } catch {
            logger.error("Failed to send \(response) status: \(error)")
        }
    }

    // MARK: - Allowance Allocation

    func allocateAllowance(for input: HandshakeInput) async throws {
        let accountId = input.hostData.statementAccountId
        logger.debug("Allocating SSS allowance...")
        do {
            try await sssManager.allocate(accountId: accountId, policy: .ignore, priority: .high)
            logger.debug("SSS allowance allocated successfully")
        } catch {
            logger.error("SSS allowance allocation failed: \(error)")
            throw error
        }
    }

    // MARK: - Statement Submission

    func submitStatement(payload: Data, input: HandshakeInput) async throws {
        let builder = StatementSubmitParametersBuilder(
            signer: StatementStoreKeypairSigner(keypair: input.signerKeypair),
            logger: logger
        )
        .addTopic1(input.topic)
        .addChannel(input.channel)
        .addScaleEncodedPayload(payload)
        .addExpiry(priorityProvider.nextPriority())

        logger.debug("Sending statement...")

        let submitter = try StatementStoreConnection(
            connection: chainRegistry.getConnectionOrError(for: chainId),
            retryMatcher: StatementSubmitOneOfErrorMatcher(
                matchers: [
                    StatementRealSubmitErrorMatcher.channelPriorityTooLow(),
                    StatementRealSubmitErrorMatcher.noAllowance(),
                    StatementSubmitTimeoutMatcher()
                ]
            ),
            logger: logger
        )
        logger.debug("Prepared submitter")

        try await submitter.submitStatement(with: builder)
        logger.debug("Posted handshake data to statement store")
    }
}
