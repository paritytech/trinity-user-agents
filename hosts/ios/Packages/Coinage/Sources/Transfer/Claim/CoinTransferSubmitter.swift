import Foundation
import ExtrinsicService
import KeyDerivation
import SubstrateSdkExt
import Individuality

/// Submits on-chain coin transfer extrinsics.
protocol CoinTransferSubmitting: Sendable {
    /// Submits a transfer extrinsic moving a coin from sender to a new destination.
    ///
    /// - Parameters:
    ///   - senderPrivateKey: Private key of the source coin owner.
    ///   - destinationCoin: Pre-allocated destination coin to transfer into.
    /// - Returns: The destination coin on success.
    func submitTransfer(senderPrivateKey: Data, destinationCoin: Coin) async throws -> Coin
}

/// Default implementation that builds and submits a `CoinagePallet.Calls.Transfer` extrinsic.
final class CoinTransferSubmitter: CoinTransferSubmitting, @unchecked Sendable {
    private let originFactory: any OriginCreating
    private let extrinsicMonitor: any ExtrinsicSubmitMonitorFactoryProtocol

    init(
        originFactory: any OriginCreating,
        extrinsicMonitor: any ExtrinsicSubmitMonitorFactoryProtocol
    ) {
        self.originFactory = originFactory
        self.extrinsicMonitor = extrinsicMonitor
    }

    func submitTransfer(senderPrivateKey: Data, destinationCoin: Coin) async throws -> Coin {
        let coinWallet = DynamicDerivedWallet(secretKeyProvider: { senderPrivateKey })
        let origin = try originFactory.createAsCoinOrigin(for: coinWallet)
        let call = CoinagePallet.Calls.Transfer(to: destinationCoin.publicKey)
        let builder: ExtrinsicBuilderClosure = { builder in
            try builder.adding(call: call.callAsFunction())
        }

        let result = try await extrinsicMonitor.submitAndMonitorWrapper(
            extrinsicBuilderClosure: builder,
            origin: origin,
            params: .empty
        ).asyncExecute()

        switch result.status {
        case .success:
            return destinationCoin
        case let .failure(error):
            throw error.error
        }
    }
}
