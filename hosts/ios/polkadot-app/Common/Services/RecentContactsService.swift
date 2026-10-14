import Foundation
import os
import Operation_iOS
import SubstrateSdk
import OperationExt
import ChainRegistry

protocol RecentContactsServiceDelegate: AnyObject {
    func recentContactsServiceDidUpdate(recentContacts: [DataProviderChange<RecentContactModelWithUsername>])
    func recentContactServiceDidFail(error: Error)
}

final class RecentContactsService: RecentContactsManaging {
    // MARK: Properties

    private weak var delegate: RecentContactsServiceDelegate?
    let recentContactsSubscriptionFactory: RecentContactsSubscriptionFactoryProtocol
    private var provider: StreamableProvider<RecentContactModel>?
    private let identityQueryFactory: IdentityPalletQueryFactoryProtocol
    private let chainRegistry: ChainRegistryProtocol
    private let usernameChainId: ChainModel.Id
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol
    private let stateLock = OSAllocatedUnfairLock()

    // MARK: Initial methods

    init(
        recentContactsSubscriptionFactory: RecentContactsSubscriptionFactoryProtocol,
        identityQueryFactory: IdentityPalletQueryFactoryProtocol,
        chainRegistry: ChainRegistryProtocol,
        usernameChainId: ChainModel.Id,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.recentContactsSubscriptionFactory = recentContactsSubscriptionFactory
        self.identityQueryFactory = identityQueryFactory
        self.chainRegistry = chainRegistry
        self.usernameChainId = usernameChainId
        self.operationQueue = operationQueue
        self.logger = logger
    }

    // MARK: Public methods

    func setup(_ delegate: RecentContactsServiceDelegate?, chainAssetID: ChainAssetId?) {
        stateLock.withLock {
            self.delegate = delegate
            subscribeToRecentContacts(for: chainAssetID)
        }
    }

    func handleAllRecentContacts(result: Result<[DataProviderChange<RecentContactModel>], any Error>) {
        processRecentContact(for: result)
    }

    func handleAllRecentContacts(
        for _: ChainAssetId,
        result: Result<[DataProviderChange<RecentContactModel>], any Error>
    ) {
        processRecentContact(for: result)
    }

    // MARK: Private methods

    private var currentDelegate: RecentContactsServiceDelegate? {
        stateLock.withLock { delegate }
    }

    private func subscribeToRecentContacts(for chainAssetID: ChainAssetId?) {
        clear(streamableProvider: &provider)

        provider =
            if let chainAssetID {
                subscribeAllRecentContacts(for: chainAssetID)
            } else {
                subscribeAllRecentContacts()
            }
    }

    private func processRecentContact(for result: Result<[DataProviderChange<RecentContactModel>], any Error>) {
        switch result {
        case let .success(contacts):
            do {
                let connection = try chainRegistry.getConnectionOrError(for: usernameChainId)
                let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: usernameChainId)
                let accountIDs = contacts.compactMap { $0.item?.accountID }
                let usernamesWrapper = identityQueryFactory.queryUsernames(
                    for: accountIDs,
                    connection: connection,
                    runtimeProvider: runtimeProvider
                )

                let assetIDs = Set(contacts.compactMap { $0.item?.chainAssetID })
                let chainAssetIDsWrapper = chainRegistry.fetchChainAssets(assetIDs: assetIDs)

                let mappingOperation = ClosureOperation<[DataProviderChange<RecentContactModelWithUsername>]> {
                    let chainAssetIDsChainModelMapping = try chainAssetIDsWrapper.targetOperation
                        .extractNoCancellableResultData()
                    let accounts = try? usernamesWrapper.targetOperation.extractNoCancellableResultData()

                    let recentContactWithUsernameComputed: (RecentContactModel)
                        -> RecentContactModelWithUsername = { item -> RecentContactModelWithUsername in
                            let username = accounts?[item.accountID]
                            let chainAsset = chainAssetIDsChainModelMapping[item.chainAssetID]

                            return RecentContactModelWithUsername(
                                recentContact: item,
                                username: username,
                                chainAsset: chainAsset
                            )
                        }

                    return contacts.map { changes in
                        switch changes {
                        case let .insert(newItem): .insert(newItem: recentContactWithUsernameComputed(newItem))
                        case let .update(newItem): .update(newItem: recentContactWithUsernameComputed(newItem))
                        case let .delete(deletedIdentifier): .delete(deletedIdentifier: deletedIdentifier)
                        }
                    }
                }
                mappingOperation.addDependency(usernamesWrapper.targetOperation)
                mappingOperation.addDependency(chainAssetIDsWrapper.targetOperation)

                let totalWrapper = chainAssetIDsWrapper
                    .insertingHead(operations: usernamesWrapper.allOperations)
                    .insertingTail(operation: mappingOperation)

                execute(
                    wrapper: totalWrapper,
                    inOperationQueue: operationQueue,
                    runningCallbackIn: .main
                ) { [weak self] result in
                    switch result {
                    case let .success(recentContacts):
                        self?.currentDelegate?.recentContactsServiceDidUpdate(recentContacts: recentContacts)
                    case let .failure(error):
                        self?.currentDelegate?.recentContactServiceDidFail(error: error)
                    }
                }
            } catch {
                currentDelegate?.recentContactServiceDidFail(error: error)
            }
        case let .failure(error):
            currentDelegate?.recentContactServiceDidFail(error: error)
        }
    }
}
