import Foundation
import os
import SubstrateSdk
import SubstrateSdkExt
import AsyncExtensions
import StructuredConcurrency
import SDKLogger
import Operation_iOS

final class RecentRecipientsProvider: @unchecked Sendable {
    private let service: RecentContactsManaging
    private let chainFormat: ChainFormat
    private let chainAssetId: ChainAssetId
    private let logger: LoggerProtocol

    init(
        service: RecentContactsManaging,
        chainFormat: ChainFormat,
        chainAssetId: ChainAssetId,
        logger: LoggerProtocol
    ) {
        self.service = service
        self.chainFormat = chainFormat
        self.chainAssetId = chainAssetId
        self.logger = logger
    }

    func subscribe() -> AnyAsyncSequence<[SearchRow<RecentContactModelWithUsername>]> {
        AsyncThrowingStream { continuation in
            let holder = AnyObjectHolder<AnyObject>()
            let cacheLock = OSAllocatedUnfairLock(
                initialState: [String: SearchRow<RecentContactModelWithUsername>]()
            )

            let delegate = RecentRecipientsProviderDelegate(
                logger: self.logger,
                onUpdate: { rows in
                    continuation.yield(rows)
                },
                updateState: { changes in
                    cacheLock.withLock { cache in
                        let merged = changes.mergeToDict(cache.mapValues(\.payload))
                        let rows = self.makeRows(from: merged)
                        cache = rows.reduce(into: [:]) { result, row in
                            result[row.payload.identifier] = row
                        }
                        return rows
                    }
                }
            )

            holder.set(delegate)

            self.service.setup(delegate, chainAssetID: self.chainAssetId)

            continuation.onTermination = { @Sendable _ in
                self.service.setup(nil, chainAssetID: self.chainAssetId)
                holder.set(nil)
            }
        }
        .eraseToAnyAsyncSequence()
    }
}

private extension RecentRecipientsProvider {
    func makeRows(
        from contacts: [String: RecentContactModelWithUsername]
    ) -> [SearchRow<RecentContactModelWithUsername>] {
        contacts.values
            .filter { $0.chainAsset != nil }
            .sorted { $0.recentContact.lastUsed > $1.recentContact.lastUsed }
            .map { makeRow(for: $0) }
    }

    func makeRow(
        for contact: RecentContactModelWithUsername
    ) -> SearchRow<RecentContactModelWithUsername> {
        var matchTerms: [String] = []

        if let username = contact.username {
            matchTerms.append(username.value)
        }

        if let address = try? contact.recentContact.accountID.toAddress(using: chainFormat) {
            matchTerms.append(address)
        }

        return SearchRow(
            accountId: contact.recentContact.accountID,
            username: contact.username,
            matchTerms: matchTerms,
            payload: contact
        )
    }
}

private final class RecentRecipientsProviderDelegate: RecentContactsServiceDelegate {
    private let logger: LoggerProtocol
    private let onUpdate: ([SearchRow<RecentContactModelWithUsername>]) -> Void
    private let updateState: ([DataProviderChange<RecentContactModelWithUsername>])
        -> [SearchRow<RecentContactModelWithUsername>]

    init(
        logger: LoggerProtocol,
        onUpdate: @escaping ([SearchRow<RecentContactModelWithUsername>]) -> Void,
        updateState: @escaping ([DataProviderChange<RecentContactModelWithUsername>])
            -> [SearchRow<RecentContactModelWithUsername>]
    ) {
        self.logger = logger
        self.onUpdate = onUpdate
        self.updateState = updateState
    }

    func recentContactsServiceDidUpdate(
        recentContacts: [DataProviderChange<RecentContactModelWithUsername>]
    ) {
        onUpdate(updateState(recentContacts))
    }

    func recentContactServiceDidFail(error: Error) {
        logger.error("Recent recipients provider failed: \(error)")
    }
}
