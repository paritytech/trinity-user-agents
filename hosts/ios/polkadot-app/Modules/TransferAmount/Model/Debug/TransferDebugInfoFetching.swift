#if TESTNET_FEATURE
    import BigInt
    import Coinage
    import SDKLogger

    protocol TransferDebugInfoFetching {
        func fetchDebugInfo(for amount: BigUInt) async throws -> TransferStrategyDebugInfo?
    }

    final class CoinageDebugInfoFetcher: TransferDebugInfoFetching {
        private let coinageService: CoinageServicing

        init(coinageService: CoinageServicing) {
            self.coinageService = coinageService
        }

        func fetchDebugInfo(for amount: BigUInt) async throws -> TransferStrategyDebugInfo? {
            let preview = try await coinageService.previewTransfer(for: amount)
            return mapToDebugInfo(preview.selectionResult)
        }
    }

    final class ExternalPaymentDebugInfoFetcher: TransferDebugInfoFetching {
        private let coinageService: CoinageServicing

        init(coinageService: CoinageServicing) {
            self.coinageService = coinageService
        }

        func fetchDebugInfo(for amount: BigUInt) async throws -> TransferStrategyDebugInfo? {
            let preview = try await coinageService.previewExternalPayment(for: amount)

            guard preview != .notEnoughBalance else {
                return nil
            }

            let voucherInfos = preview.vouchers.map { tracked in
                TransferStrategyDebugInfo.VoucherInfo(
                    derivationIndex: tracked.voucher.derivationIndex,
                    exponent: tracked.voucher.exponent
                )
            }

            let coinInfos = preview.coins.map { tracked in
                TransferStrategyDebugInfo.CoinInfo(
                    derivationIndex: tracked.coin.derivationIndex,
                    exponent: tracked.coin.exponent
                )
            }

            return TransferStrategyDebugInfo(
                strategyType: .externalPayment,
                coinsUsed: coinInfos,
                splitInfo: nil,
                vouchersToUnload: voucherInfos
            )
        }
    }

    // MARK: - CoinSelectionResult → DebugInfo

    private extension CoinageDebugInfoFetcher {
        func mapToDebugInfo(_ result: CoinSelectionResult) -> TransferStrategyDebugInfo {
            switch result {
            case let .exactMatch(coins):
                let coinInfos = coins.map { coin in
                    TransferStrategyDebugInfo.CoinInfo(
                        derivationIndex: coin.derivationIndex,
                        exponent: coin.exponent
                    )
                }
                return TransferStrategyDebugInfo(
                    strategyType: .exactMatch,
                    coinsUsed: coinInfos,
                    splitInfo: nil,
                    vouchersToUnload: []
                )

            case let .split(wholeCoins, overflowCoin, targetDenominations, changeDenominations):
                let coinInfos = wholeCoins.map { coin in
                    TransferStrategyDebugInfo.CoinInfo(
                        derivationIndex: coin.derivationIndex,
                        exponent: coin.exponent
                    )
                }
                let splitInfo = TransferStrategyDebugInfo.SplitInfo(
                    overflowCoin: TransferStrategyDebugInfo.CoinInfo(
                        derivationIndex: overflowCoin.derivationIndex,
                        exponent: overflowCoin.exponent
                    ),
                    targetDenominations: targetDenominations.map(\.exponent),
                    changeDenominations: changeDenominations.map(\.exponent)
                )
                return TransferStrategyDebugInfo(
                    strategyType: .split,
                    coinsUsed: coinInfos,
                    splitInfo: splitInfo,
                    vouchersToUnload: []
                )

            case let .unloadIntoCoins(coins, perGroupAllocations):
                let coinInfos = coins.map { coin in
                    TransferStrategyDebugInfo.CoinInfo(
                        derivationIndex: coin.derivationIndex,
                        exponent: coin.exponent
                    )
                }
                let voucherInfos = perGroupAllocations.flatMap { allocation in
                    allocation.vouchers.map { voucher in
                        TransferStrategyDebugInfo.VoucherInfo(
                            derivationIndex: voucher.derivationIndex,
                            exponent: voucher.exponent
                        )
                    }
                }
                return TransferStrategyDebugInfo(
                    strategyType: .unloadAndSplit,
                    coinsUsed: coinInfos,
                    splitInfo: nil,
                    vouchersToUnload: voucherInfos
                )
            }
        }
    }
#endif
