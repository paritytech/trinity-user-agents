import Foundation
import SubstrateSdk
import CommonService
import SDKLogger

public protocol ChainRemoteAccountDetecting: AnyObject {
    var delegate: ChainRemoteAccountDetectorDelegate? { get set }
    var callbackQueue: DispatchQueue { get set }

    func startTracking(accountId: AccountId, chain: ChainModel, connection: JSONRPCEngine) throws
    func stopTrackingAccount(for chainId: ChainModel.Id)
    func stopTrackingAll()
}

public protocol ChainRemoteAccountDetectorDelegate: AnyObject {
    func didDetectAccount(for accountId: AccountId, chain: ChainModel)
}

public final class SubstrateRemoteAccountDetector {
    public weak var delegate: ChainRemoteAccountDetectorDelegate?
    public var callbackQueue: DispatchQueue = .global()

    public let logger: SDKLoggerProtocol

    private var subscriptions: [ChainModel.Id: SyncServiceProtocol] = [:]

    public init(logger: SDKLoggerProtocol) {
        self.logger = logger
    }
}

extension SubstrateRemoteAccountDetector: ChainRemoteAccountDetecting {
    public func startTracking(accountId: AccountId, chain: ChainModel, connection: JSONRPCEngine) throws {
        guard subscriptions[chain.chainId] == nil else {
            return
        }

        let syncService = ChainRemoteAccountConfirmService(
            accountId: accountId,
            connection: connection,
            shouldConfirm: true,
            detectionClosure: { [weak self] in
                self?.delegate?.didDetectAccount(for: accountId, chain: chain)
            },
            callbackQueue: callbackQueue,
            logger: logger
        )

        subscriptions[chain.chainId] = syncService
        syncService.setup()
    }

    public func stopTrackingAccount(for chainId: ChainModel.Id) {
        subscriptions[chainId]?.stopSyncUp()
        subscriptions[chainId] = nil
    }

    public func stopTrackingAll() {
        subscriptions.forEach { $0.value.stopSyncUp() }

        subscriptions = [:]
    }
}
