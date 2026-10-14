import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription
import SubstrateSdkExt
import Combine
import OperationExt
import ChainRegistry

enum SubscriptionServiceError: Error {
    case timeOut
    case noData(StorageCodingPath)
}

protocol BaseSubscriptionServiceProtocol {}

class BaseSubscriptionService: BaseSubscriptionServiceProtocol {
    let chainRegistry: ChainRegistryProtocol
    let chain: ChainModel.Id
    let storageRequestFactory: StorageRequestFactoryProtocol
    let storageKeyFactory: StorageKeyFactoryProtocol

    let logger: LoggerProtocol?
    let operationQueue: OperationQueue

    init(
        chainRegistry: ChainRegistryProtocol,
        chain: ChainModel.Id,
        operationQueue: OperationQueue,
        logger: LoggerProtocol?
    ) {
        self.chainRegistry = chainRegistry
        self.chain = chain
        self.operationQueue = operationQueue
        self.logger = logger
        storageKeyFactory = StorageKeyFactory()
        storageRequestFactory = StorageRequestFactory(
            remoteFactory: storageKeyFactory,
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}

extension BaseSubscriptionService {
    private func ensureChainExists() -> AnyPublisher<Void, Error> {
        chainRegistry.chainsPublisher()
            .map { $0.allChangedItems() }
            .map { [chain] in $0.contains(where: { $0.chainId == chain }) }
            .filter { $0 }
            .map { _ in () }
            .setFailureType(to: Error.self)
            .timeout(30, scheduler: RunLoop.main, customError: {
                SubscriptionServiceError.timeOut
            })
            .eraseToAnyPublisher()
    }

    func batchSubscription<T: BatchStorageSubscriptionResult>(
        requests: [BatchStorageSubscriptionRequest]
    ) -> AnyPublisher<T, Error> {
        ensureChainExists()
            .flatMap { [chainRegistry, chain, operationQueue, logger] _ in
                Deferred {
                    typealias Item = T
                    var subscription: CallbackBatchStorageSubscription<Item>?

                    let publisher = PassthroughSubject<T, Error>()

                    do {
                        let connection = try chainRegistry.getConnectionOrError(for: chain)
                        let runtimeService = try chainRegistry.getRuntimeProviderOrError(for: chain)

                        subscription = CallbackBatchStorageSubscription(
                            requests: requests,
                            connection: connection,
                            runtimeService: runtimeService,
                            repository: nil,
                            operationQueue: operationQueue,
                            callbackQueue: .main
                        ) { result in
                            switch result {
                            case let .success(success):
                                publisher.send(success)
                            case let .failure(failure):
                                logger?.error(failure.localizedDescription)
                                publisher.send(completion: .failure(failure))
                            }
                        }
                        return publisher
                            .handleEvents(receiveSubscription: { _ in
                                subscription?.subscribe()
                            }, receiveCompletion: { _ in
                                subscription?.unsubscribe()
                            }, receiveCancel: {
                                subscription?.unsubscribe()
                            })
                            .eraseToAnyPublisher()
                    } catch {
                        return Fail(error: error).eraseToAnyPublisher()
                    }
                }
            }
            .eraseToAnyPublisher()
    }

    func subscription<T: Decodable>(
        request: BatchStorageSubscriptionRequest
    ) -> AnyPublisher<T, Error> {
        typealias SubscriptionType = BatchStorageSubscriptionSingleResult<T>
        let subscription: AnyPublisher<SubscriptionType, Error> = batchSubscription(requests: [request])
        return subscription.map(\.value).eraseToAnyPublisher()
    }

    func constant<T: Decodable>(
        path: any ConstantPathConvertible
    ) -> AnyPublisher<T, Error> {
        ensureChainExists()
            .flatMap { [chainRegistry, chain, operationQueue, logger] _ in
                Deferred {
                    let cancel = CancellableCallStore()
                    return Future<T, Error> { promise in
                        guard let runtimeService = chainRegistry.getRuntimeProvider(for: chain) else {
                            promise(.failure(ChainRegistryError.runtimeMetadaUnavailable))
                            return
                        }

                        let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

                        let constOperation = StorageConstantOperation<T>(
                            path: path(),
                            fallbackValue: nil
                        )
                        constOperation.configurationBlock = {
                            do {
                                constOperation.codingFactory = try codingFactoryOperation
                                    .extractNoCancellableResultData()
                            } catch {
                                constOperation.result = .failure(error)
                            }
                        }

                        constOperation.addDependency(codingFactoryOperation)
                        let wrapper = CompoundOperationWrapper(
                            targetOperation: constOperation,
                            dependencies: [codingFactoryOperation]
                        )

                        executeCancellable(
                            wrapper: wrapper,
                            inOperationQueue: operationQueue,
                            backingCallIn: cancel,
                            runningCallbackIn: .main
                        ) { result in
                            switch result {
                            case .success:
                                break
                            case let .failure(failure):
                                logger?.error(failure.localizedDescription)
                            }
                            promise(result)
                        }
                    }
                    .handleEvents(receiveCancel: {
                        cancel.cancel()
                    })
                }
            }
            .eraseToAnyPublisher()
    }

    private func ensureChainAndExecuteQuery<T>(
        at path: any StoragePathConvertible,
        setupBlock: @escaping (any StoragePathConvertible) throws -> CompoundOperationWrapper<[StorageResponse<T>]>
    ) -> AnyPublisher<T, Error> {
        ensureChainExists()
            .flatMap { [self] _ in
                createQueryPublisher(at: path, setupBlock: setupBlock)
            }
            .eraseToAnyPublisher()
    }

    func queryNumericStorage<T: LosslessStringConvertible & Equatable>(
        at path: any StoragePathConvertible,
        params: [some Encodable]
    ) -> AnyPublisher<T, Error> {
        let retVal: AnyPublisher<StringScaleMapper<T>, Error> =
            ensureChainAndExecuteQuery(at: path) { [chainRegistry, chain] path in
                let connection = try chainRegistry.getConnectionOrError(for: chain)
                return try self.createNumericStorageWrapper { factory in
                    self.storageRequestFactory.queryItems(
                        engine: connection,
                        keyParams: { params },
                        factory: factory,
                        storagePath: path()
                    )
                }
            }
        return retVal.map(\.value).eraseToAnyPublisher()
    }

    func queryNumericStorage<T: LosslessStringConvertible & Equatable>(
        at path: any StoragePathConvertible
    ) -> AnyPublisher<T, Error> {
        let retVal: AnyPublisher<StringScaleMapper<T>, Error> =
            ensureChainAndExecuteQuery(at: path) { [chainRegistry, chain] path in
                let connection = try chainRegistry.getConnectionOrError(for: chain)
                return try self.createNumericStorageWrapper { factory in
                    self.storageRequestFactory.queryItems(
                        engine: connection,
                        keys: { try [self.storageKeyFactory.key(from: path())] },
                        factory: factory,
                        storagePath: path()
                    )
                }
            }
        return retVal.map(\.value).eraseToAnyPublisher()
    }

    func queryStorage<T: Decodable>(
        at path: any StoragePathConvertible
    ) -> AnyPublisher<T, Error> {
        ensureChainAndExecuteQuery(at: path) { [chainRegistry, chain] path in
            let connection = try chainRegistry.getConnectionOrError(for: chain)
            return try self.createStorageWrapper { factory in
                self.storageRequestFactory.queryItems(
                    engine: connection,
                    keys: { try [self.storageKeyFactory.key(from: path())] },
                    factory: factory,
                    storagePath: path()
                )
            }
        }
    }

    func queryStorage<T: Decodable>(
        at path: any StoragePathConvertible,
        params: [some Encodable]
    ) -> AnyPublisher<T, Error> {
        ensureChainAndExecuteQuery(at: path) { [chainRegistry, chain] path in
            let connection = try chainRegistry.getConnectionOrError(for: chain)
            return try self.createStorageWrapper { factory in
                self.storageRequestFactory.queryItems(
                    engine: connection,
                    keyParams: { params },
                    factory: factory,
                    storagePath: path()
                )
            }
        }
    }
}

private extension BaseSubscriptionService {
    typealias FactoryProvider = () throws -> RuntimeCoderFactoryProtocol

    func createStorageWrapper<T>(
        builderBlock: @escaping (@escaping FactoryProvider) -> CompoundOperationWrapper<[StorageResponse<T>]>
    ) throws -> CompoundOperationWrapper<[StorageResponse<T>]> {
        let runtimeService = try chainRegistry.getRuntimeProviderOrError(for: chain)
        let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

        let factoryProvider = { try codingFactoryOperation.extractNoCancellableResultData() }
        let wrapper = builderBlock(factoryProvider)

        wrapper.addDependency(operations: [codingFactoryOperation])
        return wrapper.insertingHead(operations: [codingFactoryOperation])
    }

    func createNumericStorageWrapper<T: LosslessStringConvertible & Equatable>(
        builderBlock: @escaping (@escaping FactoryProvider)
            -> CompoundOperationWrapper<[StorageResponse<StringScaleMapper<T>>]>
    ) throws -> CompoundOperationWrapper<[StorageResponse<StringScaleMapper<T>>]> {
        try createStorageWrapper(builderBlock: builderBlock)
    }

    private func createQueryPublisher<T>(
        at path: any StoragePathConvertible,
        setupBlock: @escaping (any StoragePathConvertible) throws -> CompoundOperationWrapper<[StorageResponse<T>]>
    ) -> AnyPublisher<T, Error> {
        Deferred { [operationQueue, logger] in
            let cancel = CancellableCallStore()
            return Future<T, Error> { promise in
                do {
                    try executeCancellable(
                        wrapper: setupBlock(path),
                        inOperationQueue: operationQueue,
                        backingCallIn: cancel,
                        runningCallbackIn: .main
                    ) { result in
                        switch result {
                        case let .success(value) where !value.isEmpty:
                            guard let retVal = value.first?.value else {
                                promise(.failure(SubscriptionServiceError.noData(path())))
                                return
                            }
                            promise(.success(retVal))
                        case .success:
                            promise(.failure(SubscriptionServiceError.noData(path())))
                        case let .failure(error):
                            logger?.error(error.localizedDescription)
                            promise(.failure(error))
                        }
                    }
                } catch {
                    promise(.failure(error))
                }
            }
            .handleEvents(receiveCancel: {
                cancel.cancel()
            })
        }
        .eraseToAnyPublisher()
    }
}
