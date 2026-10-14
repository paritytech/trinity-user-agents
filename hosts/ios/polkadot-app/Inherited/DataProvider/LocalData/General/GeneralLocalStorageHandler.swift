import Foundation
import SubstrateSdk
import ChainRegistry

protocol SystemLocalDataHandler {
    func handleBlockNumber(
        result: Result<BlockNumber?, Error>,
        chainId: ChainModel.Id
    )

    func handleAccountInfo(
        result: Result<SystemPallet.AccountInfo?, Error>,
        accountId: AccountId,
        chainId: ChainModel.Id
    )
}

extension SystemLocalDataHandler {
    func handleBlockNumber(
        result _: Result<BlockNumber?, Error>,
        chainId _: ChainModel.Id
    ) {}

    func handleAccountInfo(
        result _: Result<SystemPallet.AccountInfo?, Error>,
        accountId _: AccountId,
        chainId _: ChainModel.Id
    ) {}
}
