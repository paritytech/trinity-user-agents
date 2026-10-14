import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

protocol SystemLocalDataSubscriber: LocalStorageProviderObserving {
    var systemLocalDataFactory: SystemLocalDataFactoryProtocol { get }

    var systemLocalDataHandler: SystemLocalDataHandler { get }

    func subscribeToBlockNumber(
        for chainId: ChainModel.Id
    ) -> AnyDataProvider<SystemLocalData.DecodedBlockNumber>?

    func subscribeAccountInfo(
        for accountId: AccountId,
        chainId: ChainModel.Id
    ) -> AnyDataProvider<SystemLocalData.DecodedAccountInfo>?
}

extension SystemLocalDataSubscriber {
    func subscribeToBlockNumber(
        for chainId: ChainModel.Id
    ) -> AnyDataProvider<SystemLocalData.DecodedBlockNumber>? {
        guard let blockNumberProvider = try? systemLocalDataFactory.getBlockNumberProvider(
            for: chainId
        ) else {
            return nil
        }

        let updateClosure = { [weak self] (changes: [DataProviderChange<SystemLocalData.DecodedBlockNumber>]) in
            let blockNumber = changes.reduceToLastChange()
            self?.systemLocalDataHandler.handleBlockNumber(
                result: .success(blockNumber?.item?.value),
                chainId: chainId
            )
        }

        let failureClosure: (Error) -> Void = { [weak self] (error: Error) in
            self?.systemLocalDataHandler.handleBlockNumber(
                result: .failure(error), chainId: chainId
            )
        }

        let options = DataProviderObserverOptions(
            alwaysNotifyOnRefresh: false,
            waitsInProgressSyncOnAdd: false
        )

        blockNumberProvider.addObserver(
            self,
            deliverOn: .main,
            executing: updateClosure,
            failing: failureClosure,
            options: options
        )

        return blockNumberProvider
    }

    func subscribeAccountInfo(
        for accountId: AccountId,
        chainId: ChainModel.Id
    ) -> AnyDataProvider<SystemLocalData.DecodedAccountInfo>? {
        guard
            let provider = try? systemLocalDataFactory.getAccountInfoProvider(
                for: accountId,
                chainId: chainId
            )
        else {
            return nil
        }

        addDataProviderObserver(
            for: provider,
            updateClosure: { [weak self] accountInfo in
                self?.systemLocalDataHandler.handleAccountInfo(
                    result: .success(accountInfo),
                    accountId: accountId,
                    chainId: chainId
                )
            },
            failureClosure: { [weak self] error in
                self?.systemLocalDataHandler.handleAccountInfo(
                    result: .failure(error),
                    accountId: accountId,
                    chainId: chainId
                )
            }
        )

        return provider
    }
}

extension SystemLocalDataSubscriber where Self: SystemLocalDataHandler {
    var systemLocalDataHandler: SystemLocalDataHandler { self }
}
