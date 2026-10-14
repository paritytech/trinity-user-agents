import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription

public struct NMapSubscriptionRequest<Key: NMapKeyStorageKeyProtocol>: SubscriptionRequestProtocol {
    public let storagePath: StorageCodingPath
    public let localKey: String
    public let keyParamClosure: () throws -> Key

    public init(
        storagePath: StorageCodingPath,
        localKey: String,
        keyParamClosure: @escaping () throws -> Key
    ) {
        self.storagePath = storagePath
        self.localKey = localKey
        self.keyParamClosure = keyParamClosure
    }

    public func createKeyEncodingWrapper(
        using storageKeyFactory: StorageKeyFactoryProtocol,
        codingFactoryClosure: @escaping () throws -> RuntimeCoderFactoryProtocol
    ) -> CompoundOperationWrapper<Data> {
        let operation = NMapKeyEncodingOperation(
            path: storagePath,
            storageKeyFactory: storageKeyFactory
        )

        operation.configurationBlock = { [weak operation] in
            do {
                operation?.keys = try [keyParamClosure()]
                operation?.codingFactory = try codingFactoryClosure()
            } catch {
                operation?.result = .failure(error)
            }
        }

        let closureOperation = ClosureOperation<Data> {
            let result = try operation.extractNoCancellableResultData()
            guard let data = result.first else {
                throw BaseOperationError.unexpectedDependentResult
            }
            return data
        }

        closureOperation.addDependency(operation)

        return CompoundOperationWrapper(targetOperation: closureOperation, dependencies: [operation])
    }
}
