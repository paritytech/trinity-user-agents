import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import ChainStore
import StructuredConcurrency

public protocol MembershipProofParamsFetching {
    func fetch(
        for ringIndex: MembersPallet.RingIndex,
        collectionId: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersProofParams?

    func fetchCurrentRevision(
        for ringIndex: MembersPallet.RingIndex,
        collectionId: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersPallet.RevisionIndex?
}

public enum MembershipProofParamsError: Error {
    case noParams
}

public extension MembershipProofParamsFetching {
    func fetchOrError(
        for ringIndex: MembersPallet.RingIndex,
        collectionId: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersProofParams {
        let optParams = try await fetch(
            for: ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        )

        guard let params = optParams else {
            throw MembershipProofParamsError.noParams
        }

        return params
    }
}

public final class MembershipProofParamsFetcher {
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol

    private let requestFactory = StorageRequestFactory.asyncInit()

    public init(
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol
    ) {
        self.connection = connection
        self.runtimeCodingService = runtimeCodingService
    }
}

private extension MembershipProofParamsFetcher {
    func fetchRingKeys(
        identifier: MembersPallet.CollectionIdentifier,
        ringIndex: MembersPallet.RingIndex,
        blockHash: Data?,
        codingFactory: RuntimeCoderFactoryProtocol
    ) async throws -> MembersPallet.RingKeys? {
        async let allPagesResult: [RingPageKey: [BytesCodable]] = try await requestFactory
            .queryByPrefix(
                engine: connection,
                request: DoubleMapRemoteStorageRequest(
                    storagePath: MembersPallet.Storage.ringKeys(),
                    keyParamClosure: {
                        (
                            BytesCodable(wrappedValue: identifier),
                            StringCodable(wrappedValue: ringIndex)
                        )
                    }
                ),
                storagePath: MembersPallet.Storage.ringKeys(),
                factory: { codingFactory },
                options: StorageQueryListOptions(atBlock: blockHash)
            )
            .asyncExecute()

        async let ringStatus: MembersPallet.RingStatus? = requestFactory.queryItems(
            engine: connection,
            keyParams1: {
                [BytesCodable(wrappedValue: identifier)]
            },
            keyParams2: {
                [StringCodable(wrappedValue: ringIndex)]
            },
            factory: { codingFactory },
            storagePath: MembersPallet.Storage.ringKeysStatus(),
            options: StorageQueryListOptions(atBlock: blockHash)
        )
        .asyncExecute()
        .first?.value

        guard let status = try await ringStatus else {
            return nil
        }

        let members = try await allPagesResult
            .sorted { $0.key.pageIndex < $1.key.pageIndex }
            .flatMap(\.value)

        return MembersPallet.RingKeys(
            allMembers: members.map(\.wrappedValue),
            includedCount: status.included
        )
    }
}

extension MembershipProofParamsFetcher: MembershipProofParamsFetching {
    public func fetch(
        for ringIndex: MembersPallet.RingIndex,
        collectionId: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersProofParams? {
        let codingFactory = try await runtimeCodingService.fetchCoderFactoryOperation().asyncExecute()

        async let ringKeys = fetchRingKeys(
            identifier: collectionId,
            ringIndex: ringIndex,
            blockHash: blockHash,
            codingFactory: codingFactory
        )

        async let collection: MembersPallet.CollectionInfo? = requestFactory.queryItems(
            engine: connection,
            keyParams: { [BytesCodable(wrappedValue: collectionId)] },
            factory: { codingFactory },
            storagePath: MembersPallet.Storage.collections(),
            at: blockHash
        )
        .asyncExecute()
        .first?.value

        async let revision: MembersPallet.RevisionIndex? = fetchCurrentRevision(
            for: ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        )

        guard
            let keys = try await ringKeys,
            let ringSize = try await collection?.ringSize,
            let revision = try await revision
        else {
            return nil
        }

        return MembersProofParams(
            ringMembers: keys.includedMembers,
            ringSize: ringSize.domainSize,
            revision: revision
        )
    }

    public func fetchCurrentRevision(
        for ringIndex: MembersPallet.RingIndex,
        collectionId: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersPallet.RevisionIndex? {
        let codingFactory = try await runtimeCodingService.fetchCoderFactoryOperation().asyncExecute()

        let root: MembersPallet.RingRoot? = try await requestFactory.queryItems(
            engine: connection,
            keyParams1: { [BytesCodable(wrappedValue: collectionId)] },
            keyParams2: { [StringCodable(wrappedValue: ringIndex)] },
            factory: { codingFactory },
            storagePath: MembersPallet.Storage.root(),
            options: StorageQueryListOptions(atBlock: blockHash)
        )
        .asyncExecute()
        .first?.value

        return root?.revision
    }
}

private extension MembershipProofParamsFetcher {
    struct RingPageKey: JSONListConvertible, Hashable {
        let identifier: MembersPallet.CollectionIdentifier
        let ringIndex: MembersPallet.RingIndex
        let pageIndex: MembersPallet.PageIndex

        init(
            identifier: MembersPallet.CollectionIdentifier,
            ringIndex: MembersPallet.RingIndex,
            pageIndex: MembersPallet.PageIndex
        ) {
            self.identifier = identifier
            self.ringIndex = ringIndex
            self.pageIndex = pageIndex
        }

        init(jsonList: [JSON], context: [CodingUserInfoKey: Any]?) throws {
            guard jsonList.count == 3 else {
                throw JSONListConvertibleError.unexpectedNumberOfItems(
                    expected: 3,
                    actual: jsonList.count
                )
            }

            identifier = try jsonList[0].map(
                to: BytesCodable.self,
                with: context
            ).wrappedValue

            ringIndex = try jsonList[1].map(
                to: StringScaleMapper<MembersPallet.RingIndex>.self,
                with: context
            ).value

            pageIndex = try jsonList[2].map(
                to: StringScaleMapper<MembersPallet.PageIndex>.self,
                with: context
            ).value
        }
    }
}
