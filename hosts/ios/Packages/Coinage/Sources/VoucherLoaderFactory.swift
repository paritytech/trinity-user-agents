import Foundation
import ExtrinsicService
import KeyDerivation
import SDKLogger
import SubstrateSdk

public protocol VoucherLoaderFactoryProtocol {
    func makeLoader(for externalAssetHolder: any WalletManaging) throws -> VoucherLoaderProtocol
}

final class VoucherLoaderFactory: VoucherLoaderFactoryProtocol {
    private let instanceId: CoinageInstanceId
    private let minter: any VoucherMinting
    private let keypairFactory: any VoucherKeyDeriving
    private let txService: any CoinageTxServicing
    private let originCreating: OriginCreating
    private let runtimeService: RuntimeCodingServiceProtocol
    private let chain: ChainProtocol
    private let logger: (any SDKLoggerProtocol)?

    init(
        instanceId: CoinageInstanceId,
        minter: any VoucherMinting,
        keypairFactory: any VoucherKeyDeriving,
        txService: any CoinageTxServicing,
        originCreating: OriginCreating,
        runtimeService: RuntimeCodingServiceProtocol,
        chain: ChainProtocol,
        logger: (any SDKLoggerProtocol)?
    ) {
        self.instanceId = instanceId
        self.minter = minter
        self.keypairFactory = keypairFactory
        self.txService = txService
        self.originCreating = originCreating
        self.runtimeService = runtimeService
        self.chain = chain
        self.logger = logger
    }

    func makeLoader(for externalAssetHolder: any WalletManaging) throws -> VoucherLoaderProtocol {
        let account = try externalAssetHolder.fetchAccount(for: chain)
        let origin = try originCreating.createInfallibleUnpaidSignedOrigin(for: externalAssetHolder)

        return VoucherLoader(
            instanceId: instanceId,
            accountId: account.accountId,
            origin: origin,
            minter: minter,
            keypairFactory: keypairFactory,
            txService: txService,
            runtimeService: runtimeService,
            logger: logger
        )
    }
}
