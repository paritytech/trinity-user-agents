import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery

/// Records the one query the caller makes — its path and the keys it asks for — and fails it, since a
/// `StorageResponse` cannot be built outside the SDK.
final class StubStorageRequestFactory: StorageRequestFactoryProtocol {
    struct Query: Equatable {
        let path: StorageCodingPath
        let keys: [BytesCodable]
    }

    private(set) var queries: [Query] = []

    func queryItems<T>(
        engine _: JSONRPCEngine,
        keyParams: @escaping () throws -> [some Encodable],
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        storagePath: StorageCodingPath,
        options _: StorageQueryListOptions
    ) -> CompoundOperationWrapper<[StorageResponse<T>]> where T: Decodable {
        let keys = (try? keyParams())?.compactMap { $0 as? BytesCodable } ?? []
        queries.append(Query(path: storagePath, keys: keys))
        return CompoundOperationWrapper.createWithError(StubError.storageUnavailable)
    }

    func queryItem<T>(
        engine _: JSONRPCEngine,
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        storagePath _: StorageCodingPath,
        at _: Data?
    ) -> CompoundOperationWrapper<StorageResponse<T>> where T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryItems<T>(
        engine _: JSONRPCEngine,
        keyParams1 _: @escaping () throws -> [some Encodable],
        keyParams2 _: @escaping () throws -> [some Encodable],
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        storagePath _: StorageCodingPath,
        options _: StorageQueryListOptions
    ) -> CompoundOperationWrapper<[StorageResponse<T>]> where T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryNMapItems<T>(
        engine _: JSONRPCEngine,
        nParamKeys _: @escaping () throws -> [NMapKeyStorageKeyProtocol],
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        storagePath _: StorageCodingPath,
        options _: StorageQueryListOptions
    ) -> CompoundOperationWrapper<[StorageResponse<T>]> where T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryItems<T>(
        engine _: JSONRPCEngine,
        keys _: @escaping () throws -> [Data],
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        storagePath _: StorageCodingPath,
        options _: StorageQueryListOptions
    ) -> CompoundOperationWrapper<[StorageResponse<T>]> where T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryChildItem<T>(
        engine _: JSONRPCEngine,
        storageKeyParam _: @escaping () throws -> Data,
        childKeyParam _: @escaping () throws -> Data,
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        mapper _: DynamicScaleDecodable,
        at _: Data?
    ) -> CompoundOperationWrapper<ChildStorageResponse<T>> where T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryByPrefix<K, T>(
        engine _: JSONRPCEngine,
        request _: RemoteStorageRequestProtocol,
        storagePath _: StorageCodingPath,
        factory _: @escaping () throws -> RuntimeCoderFactoryProtocol,
        options _: StorageQueryListOptions
    ) -> CompoundOperationWrapper<[K: T]> where K: JSONListConvertible, T: Decodable {
        CompoundOperationWrapper.createWithError(StubError.unused)
    }

    func queryRawItems(
        for _: @escaping () throws -> [Data],
        at _: Data?,
        engine _: JSONRPCEngine
    ) -> BaseOperation<[[StorageUpdate]]> {
        ClosureOperation { throw StubError.unused }
    }
}
