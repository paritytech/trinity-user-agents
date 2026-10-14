import AsyncExtensions
import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription
import StructuredConcurrency
import BigInt
import SDKLogger
import XcmDefinition

public typealias CoinageAssetLocationId = Xcm.Version5<XcmUni.AssetId>

public protocol AssetsTracking: Sendable {
    func track(
        instanceId: CoinageInstanceId,
        accountId: AccountId
    ) async throws -> AnyAsyncSequence<Balance>
}

public final class AssetBalanceTracker: AssetsTracking, @unchecked Sendable {
    private let connection: any JSONRPCEngine
    private let runtimeService: RuntimeCodingServiceProtocol
    private let storageRequestFactory: any StorageRequestFactoryProtocol
    private let logger: SDKLoggerProtocol?

    public init(
        connection: any JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        storageRequestFactory: any StorageRequestFactoryProtocol,
        logger: SDKLoggerProtocol?
    ) {
        self.connection = connection
        self.runtimeService = runtimeService
        self.storageRequestFactory = storageRequestFactory
        self.logger = logger
    }

    public func track(
        instanceId: CoinageInstanceId,
        accountId: AccountId
    ) async throws -> AnyAsyncSequence<Balance> {
        do {
            let assetId = try await resolveAssetId(instanceId: instanceId)

            logger?.debug(
                "Starting tracking: \(assetId) \(String(describing: try? accountId.toAddress(using: .genericFormat)))"
            )

            let request = BatchStorageSubscriptionRequest(
                innerRequest: DoubleMapSubscriptionRequest(
                    storagePath: AssetsPallet.Storage.account(),
                    localKey: "",
                    keyParamClosure: {
                        (
                            assetId,
                            BytesCodable(wrappedValue: accountId)
                        )
                    }
                ),
                mappingKey: Self.balanceMappingKey
            )

            return CallbackBatchStorageSubscription.asyncStream(
                requests: [request],
                connection: connection,
                runtimeService: runtimeService,
                logger: logger
            )
            .map { (result: AssetsBalanceResult) in result.balance }
            .eraseToAnyAsyncSequence()
        } catch {
            logger?.error("Failed to receive asset id for instance \(instanceId): \(error)")
            throw error
        }
    }
}

private extension AssetBalanceTracker {
    static let balanceMappingKey = "balance"

    /// Reads the coinage instance's underlying pallet-assets asset id from `Instances`. Uses a
    /// dedicated partial-decode struct (not the shared `InstanceRecord`) so a wrong field-name guess
    /// can never regress denomination loading.
    func resolveAssetId(instanceId: CoinageInstanceId) async throws -> CoinageAssetLocationId {
        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        let record: CoinageInstanceAsset? = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams: { [instanceId] in [StringCodable(wrappedValue: instanceId)] },
            factory: { coderFactory },
            storagePath: CoinagePallet.Storage.instances(),
            at: nil
        )
        .asyncExecute()
        .first?
        .value

        guard let record else {
            throw CoinageError.notConfigured
        }

        return record.assetId
    }
}

private struct CoinageInstanceAsset: Decodable {
    let assetId: CoinageAssetLocationId
}

/// Single-key `Assets.Account` subscription result. An absent account row means a zero balance.
struct AssetsBalanceResult: BatchStorageSubscriptionResult {
    let balance: Balance

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson _: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        let account = try values.first?.value.map(to: AssetsPallet.Account?.self, with: context) ?? nil
        balance = account?.balance ?? 0
    }
}
