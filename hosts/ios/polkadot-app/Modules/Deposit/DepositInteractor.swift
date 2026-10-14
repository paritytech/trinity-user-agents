import UIKit
import SubstrateSdk
import Operation_iOS
import ChainRegistry
import SubstrateSdkExt

final class DepositInteractor {
    weak var presenter: DepositInteractorOutputProtocol?

    private let depositAsset: ChainAsset
    private let fundedAsset: ChainAsset
    private let depositService: DepositServiceProtocol
    private let qrEncoder: AddressQREncodable
    private let qrFactory: QRCreationOperationFactoryProtocol
    private let chainRegistry: ChainRegistryProtocol
    private let logger: LoggerProtocol

    private var fetchDepositTask: Task<Void, Never>?
    private var monitorExecutionsTask: Task<Void, Never>?

    init(
        depositService: DepositServiceProtocol,
        depositAsset: ChainAsset,
        fundedAsset: ChainAsset,
        chainRegistry: ChainRegistryProtocol,
        qrEncoder: AddressQREncodable,
        qrFactory: QRCreationOperationFactoryProtocol = QRCreationOperationFactory(chainStyle: nil),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.depositService = depositService
        self.depositAsset = depositAsset
        self.fundedAsset = fundedAsset
        self.chainRegistry = chainRegistry
        self.qrEncoder = qrEncoder
        self.qrFactory = qrFactory
        self.logger = logger
    }

    deinit {
        fetchDepositTask?.cancel()
        monitorExecutionsTask?.cancel()
    }
}

extension DepositInteractor: DepositInteractorInputProtocol {
    func setup() {
        provideDepositState()
        subscribeExecutions()
    }
}

private extension DepositInteractor {
    enum Constants {
        static let qrSize = CGSize(width: 200, height: 200)
    }

    func provideDepositState() {
        fetchDepositTask = Task { [weak self] in
            do {
                guard
                    let assetId = self?.depositAsset.chainAssetId,
                    let info = try await self?.depositService.fetchDepositInfo(
                        for: assetId
                    ) else {
                    return
                }

                guard let summary = try await self?.createSummary(for: info) else {
                    return
                }

                await self?.presenter?.didReceive(depositSummary: summary)
            } catch {
                self?.logger.error("Deposit info fetch failed: \(error)")
            }
        }
    }

    func createSummary(for serviceInfo: DepositServiceInfo) async throws -> DepositSummary {
        let account = try serviceInfo.walletToDeposit.fetchAccount(for: depositAsset.chain)
        let address = try account.accountId.toAddress(using: depositAsset.chain.chainFormat)
        let qrPayload = try qrEncoder.encode(address: address)

        let rate = try Decimal.rateFromSubstrate(
            amount1: serviceInfo.amountIn,
            amount2: serviceInfo.amountOut,
            precision1: depositAsset.assetDisplayInfo.assetPrecision,
            precision2: fundedAsset.assetDisplayInfo.assetPrecision
        )

        let qrWrapper = qrFactory.createOperation(
            payload: qrPayload,
            qrSize: Constants.qrSize
        )

        let qrCode = try await qrWrapper.asyncExecute()

        return DepositSummary(
            depositAddress: address,
            minimumAmount: serviceInfo.minDeposit,
            rate: rate,
            feeInUsd: serviceInfo.feeInUsd,
            qrCode: qrCode
        )
    }

    func subscribeExecutions() {
        monitorExecutionsTask = Task { [weak self] in
            guard let stream = await self?.depositService.executions() else {
                return
            }

            do {
                for try await items in stream {
                    guard let self else {
                        break
                    }

                    do {
                        try await handleExecutions(items)
                    } catch {
                        logger.error("Unexpected execution handling error: \(error)")
                    }
                }
            } catch {
                self?.logger.error("Unexpected execution stream error: \(error)")
            }
        }
    }

    func handleExecutions(_ executions: [DepositExecutionItem]) async throws {
        let operations = try executions.map { execution in
            let chain = try chainRegistry.getChainOrError(
                for: execution.execLabel.chainAssetId.chainId
            )

            let chainAsset = try chain.chainAssetOrError(
                for: execution.execLabel.chainAssetId.assetId
            )

            return DepositOperationModel(assetIn: chainAsset, execution: execution)
        }

        await presenter?.didReceive(operations: operations)
    }
}
