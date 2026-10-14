import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

enum SystemLocalData {
    typealias DecodedBlockNumber = ChainStorageDecodedItem<StringScaleMapper<BlockNumber>>
    typealias DecodedAccountInfo = ChainStorageDecodedItem<SystemPallet.AccountInfo>
}

protocol SystemLocalDataFactoryProtocol {
    func getBlockNumberProvider(
        for chainId: ChainModel.Id
    ) throws -> AnyDataProvider<SystemLocalData.DecodedBlockNumber>

    func getAccountInfoProvider(
        for accountId: AccountId,
        chainId: ChainModel.Id
    ) throws -> AnyDataProvider<SystemLocalData.DecodedAccountInfo>
}

final class SystemLocalDataFactory: SubstrateLocalSubscriptionFactory, SystemLocalDataFactoryProtocol {
    func getBlockNumberProvider(
        for chainId: ChainModel.Id
    ) throws -> AnyDataProvider<SystemLocalData.DecodedBlockNumber> {
        let codingPath = SystemPallet.blockNumberPath
        let localKey = try LocalStorageKeyFactory().createFromStoragePath(codingPath, chainId: chainId)

        return try getDataProvider(
            for: localKey,
            chainId: chainId,
            storageCodingPath: codingPath,
            shouldUseFallback: false
        )
    }

    func getAccountInfoProvider(
        for accountId: AccountId,
        chainId: ChainModel.Id
    ) throws -> AnyDataProvider<SystemLocalData.DecodedAccountInfo> {
        let codingPath = SystemPallet.accountPath

        let localKey = try LocalStorageKeyFactory().createFromStoragePath(
            codingPath,
            accountId: accountId,
            chainId: chainId
        )

        return try getDataProvider(
            for: localKey,
            chainId: chainId,
            storageCodingPath: codingPath,
            shouldUseFallback: false
        )
    }
}
