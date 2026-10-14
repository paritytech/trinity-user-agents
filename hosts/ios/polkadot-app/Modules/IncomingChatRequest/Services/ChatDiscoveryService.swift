import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import MessageExchangeKit
import StatementStore
import AsyncExtensions
import AsyncAlgorithms
import SDKLogger
import SubstrateSdkExt

protocol ChatDiscoveryServicing {
    func makeDiscoveryTask(for ownKeyId: Chat.Contact.Own) -> AnyAsyncSequence<ChatRequest.ValidatedRemoteModel>?
}

enum ChatDiscoveryServiceValidationError: Error {
    case noPayload
}

final class ChatDiscoveryService {
    struct TaskParams {
        let ownKeyId: Chat.Contact.Own
        let ownAccountId: AccountId
    }

    private let signManager: StatementStoreSignerManaging
    private let statementStoreConnection: StatementStoreConnecting
    private let settings: ChatDiscoverySettingsStoring
    private let chatRequestFactory: ChatRequestFactoryProtocol
    private let logger: SDKLoggerProtocol

    private let pollDispatchQueue = DispatchQueue(label: "io.discovery.chat.poll.queue")
    private let pollOperationQueue = OperationQueue()

    private enum Constants {
        static let maxLookbackDays: ChatRequest.Day = 7
    }

    init(
        signManager: StatementStoreSignerManaging,
        settings: ChatDiscoverySettingsStoring,
        statementStoreConnection: StatementStoreConnecting,
        chatRequestFactory: ChatRequestFactoryProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.signManager = signManager
        self.settings = settings
        self.statementStoreConnection = statementStoreConnection
        self.chatRequestFactory = chatRequestFactory
        self.logger = logger
    }
}

extension ChatDiscoveryService: ChatDiscoveryServicing {
    func makeDiscoveryTask(for ownKeyId: Chat.Contact.Own) -> AnyAsyncSequence<ChatRequest.ValidatedRemoteModel>? {
        guard let pagination = ChatRequest.paginationDay(from: Date()) else {
            logger.error("Can't create pagination")
            return nil
        }

        let stream = AsyncStream<ChatRequest.ValidatedRemoteModel> { continuation in
            let task = self.performSetup(for: ownKeyId, currentDay: pagination.day) { request in
                continuation.yield(request)
            }

            continuation.onTermination = { _ in
                task.cancel()
            }
        }

        return stream.eraseToAnyAsyncSequence()
    }
}

private extension ChatDiscoveryService {
    func performSetup(
        for ownKeyId: Chat.Contact.Own,
        currentDay: ChatRequest.Day,
        onRequest: @escaping (ChatRequest.ValidatedRemoteModel) -> Void
    ) -> Task<Void, Never> {
        Task { [weak self, signManager] in
            do {
                let ownAccountId = try signManager.makeSigner(for: ownKeyId.signKeyId).accountId
                await self?.performSync(
                    for: .init(ownKeyId: ownKeyId, ownAccountId: ownAccountId),
                    currentDay: currentDay,
                    onRequest: onRequest
                )

                let params = TaskParams(ownKeyId: ownKeyId, ownAccountId: ownAccountId)
                await self?.makePollingAndAutoswitchDays(for: params, onRequest: onRequest)

                self?.logger.debug("Completed task")
            } catch {
                self?.logger.error("Discovery task failed: \(error)")
            }
        }
    }
}

// MARK: - Initial Sync

private extension ChatDiscoveryService {
    func performSync(
        for params: TaskParams,
        currentDay: ChatRequest.Day,
        onRequest: (ChatRequest.ValidatedRemoteModel) -> Void
    ) async {
        if await performFullSync(for: params, onRequest: onRequest) {
            settings.saveLastChatRequestDay(currentDay, for: params.ownAccountId)
            return
        }

        if await performDayBasedSync(
            for: params,
            currentDay: currentDay,
            onRequest: onRequest
        ) {
            settings.saveLastChatRequestDay(currentDay, for: params.ownAccountId)
            return
        }
    }

    func performFullSync(
        for params: TaskParams,
        onRequest: (ChatRequest.ValidatedRemoteModel) -> Void
    ) async -> Bool {
        do {
            logger.debug("Starting full sync")
            let items = try await fetchForPeer(for: params)

            items.forEach { onRequest($0) }

            logger.debug("Performed full sync")

            return true
        } catch {
            logger.error("Full sync failed: \(error)")
            return false
        }
    }

    func performDayBasedSync(
        for params: TaskParams,
        currentDay: ChatRequest.Day,
        onRequest: (ChatRequest.ValidatedRemoteModel) -> Void
    ) async -> Bool {
        do {
            let startDay =
                if
                    let lastSyncDay = settings.fetchLastChatRequestDay(for: params.ownAccountId),
                    lastSyncDay <= currentDay {
                    max(lastSyncDay, currentDay - Constants.maxLookbackDays)
                } else {
                    currentDay - Constants.maxLookbackDays
                }

            let syncDays = (startDay ..< currentDay).reversed()

            guard !syncDays.isEmpty else {
                logger.debug("No days to sync")
                return true
            }

            logger.debug("Will start days based sync: \(syncDays.count)")

            for day in syncDays {
                guard !Task.isCancelled else {
                    return false
                }

                logger.debug("Start sync for day: \(day)")
                let items = try await fetchDay(day, params: params)

                items.forEach { onRequest($0) }

                logger.debug("Synced for day \(day): \(items.count)")
            }

            logger.debug("Did complete days based sync: \(syncDays.count)")

            return true
        } catch {
            logger.error("Days based sync failed: \(error)")
            return false
        }
    }

    func makePollingAndAutoswitchDays(
        for params: TaskParams,
        onRequest: @escaping (ChatRequest.ValidatedRemoteModel) -> Void
    ) async {
        var currentDayPoller: StatementSubscription?

        while true {
            currentDayPoller?.stop()

            guard !Task.isCancelled else {
                return
            }

            guard let pagination = ChatRequest.paginationDay(from: Date()) else {
                return
            }

            do {
                currentDayPoller = try preparePollerForDay(pagination.day, params: params)

                currentDayPoller?.start { [weak self] statement in
                    guard let self else {
                        return false
                    }

                    Task {
                        do {
                            let model = try await self.handleStatement(statement, params: params)
                            onRequest(model)
                        } catch {
                            self.logger.error("Failed statement handling: \(error)")
                        }
                    }

                    return true
                }

                if pagination.remainedTillNext > 0 {
                    let delay = UInt64(TimeInterval(NSEC_PER_SEC) * pagination.remainedTillNext)

                    logger.debug("Remained till next day: \(delay)")
                    try await Task.sleep(nanoseconds: delay)
                }
            } catch {
                logger.error("Unexpected poller failure: \(error)")
                return
            }
        }
    }

    func fetchForPeer(for params: TaskParams) async throws -> [ChatRequest.ValidatedRemoteModel] {
        let poller = try preparePollerForPeer(for: params)

        return try await fetchWithPoller(poller, params: params)
    }

    func fetchDay(_ day: ChatRequest.Day, params: TaskParams) async throws -> [ChatRequest.ValidatedRemoteModel] {
        let poller = try preparePollerForDay(day, params: params)

        return try await fetchWithPoller(poller, params: params)
    }

    func fetchWithPoller(
        _ poller: StatementSubscription,
        params: TaskParams
    ) async throws -> [ChatRequest.ValidatedRemoteModel] {
        let statements: [Statement] = try await withCheckedThrowingContinuation { [weak self] continuation in
            var collected: [Statement] = []

            poller.fetchOnce(handler: { statement in
                collected.append(statement)
                return true
            }, completion: { error in
                guard let error else {
                    self?.logger.debug("Completed fetch")
                    continuation.resume(returning: collected)
                    return
                }

                self?.logger.error("Failed fetch: \(error)")
                continuation.resume(throwing: error)
            })
        }

        var models: [ChatRequest.ValidatedRemoteModel] = []

        for statement in statements {
            do {
                logger.debug("Handling statement during sync")

                let model = try await handleStatement(statement, params: params)
                models.append(model)

                let address = try model.peerAccountId.toAddress(using: .substrate(42))
                logger.debug("Handled statement from \(address)")
            } catch {
                logger.error("Can't handle statement during sync: \(error)")
            }
        }

        return models
    }

    func preparePollerForPeer(for params: TaskParams) throws -> StatementSubscription {
        let topic = try ChatRequest.allPeerStatementsTopic(from: params.ownAccountId)

        return try preparePoller(for: topic)
    }

    func preparePollerForDay(_ day: ChatRequest.Day, params: TaskParams) throws -> StatementSubscription {
        let topic = try ChatRequest.paginationTopic(from: params.ownAccountId, day: day)

        return try preparePoller(for: topic)
    }

    func preparePoller(for topic: Data) throws -> StatementSubscription {
        let proofVerifier = StatementPermissiveProofVerifier()
        let rawTopic = try topic.fixedStatementFieldData()

        return StatementSubscription(
            connection: statementStoreConnection,
            topicFilter: .matchAll([rawTopic]),
            proofVerifier: proofVerifier,
            workQueue: pollDispatchQueue,
            logger: logger
        )
    }

    func handleStatement(_ statement: Statement, params: TaskParams) async throws -> ChatRequest.ValidatedRemoteModel {
        guard let scaleEncodedPayload = statement.getScaleEncodedPayload() else {
            logger.warning("Statement missing payload")
            throw ChatDiscoveryServiceValidationError.noPayload
        }

        let payloadDecoder = try ScaleDecoder(data: scaleEncodedPayload)
        let payload = try Data(scaleDecoder: payloadDecoder)

        return try await chatRequestFactory.decodeAndValidate(remotePayload: payload, ownKeyId: params.ownKeyId)
    }
}
