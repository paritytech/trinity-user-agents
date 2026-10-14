import Foundation
import SubstrateSdk
import BigInt
import AssetsManagement
import ChainRegistry

enum AssetStorageInfoError: Error {
    case unexpectedTypeExtras
    case unexpectedType(String?)
}

struct OrmlTokenStorageInfo {
    let currencyId: JSON
    let currencyData: Data
    let module: String
    let existentialDeposit: BigUInt
    let canTransferAll: Bool
}

struct NativeTokenStorageInfo {
    let canTransferAll: Bool
    let transferCallPath: CallCodingPath
}

struct AssetsPalletStorageInfo {
    let assetId: JSON
    let assetIdString: String
    let palletName: String?
}

enum AssetStorageInfo {
    case native(info: NativeTokenStorageInfo)
    case statemine(info: AssetsPalletStorageInfo)
    case orml(info: OrmlTokenStorageInfo)
    case ormlHydrationEvm(info: OrmlTokenStorageInfo)
}

extension AssetStorageInfo {
    static func extract(
        from asset: AssetModel,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> AssetStorageInfo {
        switch AssetType(rawType: asset.type) {
        case .orml:
            guard let extras = try asset.typeExtras?.map(to: OrmlTokenExtras.self) else {
                throw AssetStorageInfoError.unexpectedTypeExtras
            }

            let info = try createOrmlStorageInfo(from: extras, codingFactory: codingFactory)

            return .orml(info: info)
        case .statemine:
            guard let extras = try asset.typeExtras?.map(to: StatemineAssetExtras.self) else {
                throw AssetStorageInfoError.unexpectedTypeExtras
            }

            let assetId = try AssetsPalletSerializer.decode(
                assetId: extras.assetId,
                palletName: extras.palletName,
                codingFactory: codingFactory
            )

            let info = AssetsPalletStorageInfo(
                assetId: assetId,
                assetIdString: extras.assetId,
                palletName: extras.palletName
            )

            return .statemine(info: info)
        case .native:
            let canTransferAll = codingFactory.hasCall(for: BalancesPallet.TransferAll.codingPath)

            let transferCallPath: CallCodingPath = codingFactory.hasCall(
                for: BalancesPallet.transferAllowDeathCallPath
            ) ? BalancesPallet.transferAllowDeathCallPath : BalancesPallet.transferCallPath

            let info = NativeTokenStorageInfo(canTransferAll: canTransferAll, transferCallPath: transferCallPath)

            return .native(info: info)
        case .ormlHydrationEvm:
            let info = try createOrmlHydrationEvmStorageInfo(
                from: asset,
                codingFactory: codingFactory
            )
            return .ormlHydrationEvm(info: info)
        case .none:
            throw AssetStorageInfoError.unexpectedType(asset.type)
        }
    }

    private static func createOrmlHydrationEvmStorageInfo(
        from asset: AssetModel,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> OrmlTokenStorageInfo {
        guard let extras = try asset.typeExtras?.map(to: OrmlTokenExtras.self) else {
            throw AssetStorageInfoError.unexpectedTypeExtras
        }

        let rawCurrencyId = try Data(hexString: extras.currencyIdScale)

        let decoder = try codingFactory.createDecoder(from: rawCurrencyId)
        let currencyId = try decoder.read(type: extras.currencyIdType)

        let moduleName = OrmlPallet.currencies
        let transferAllPath = OrmlPallet.TransferAll.codingPath(for: moduleName)

        let existentialDeposit = BigUInt(extras.existentialDeposit) ?? 0

        let canTransferAll = codingFactory.metadata.getCall(
            from: transferAllPath.moduleName,
            with: transferAllPath.callName
        ) != nil

        return OrmlTokenStorageInfo(
            currencyId: currencyId,
            currencyData: rawCurrencyId,
            module: moduleName,
            existentialDeposit: existentialDeposit,
            canTransferAll: canTransferAll
        )
    }

    private static func createOrmlStorageInfo(
        from extras: OrmlTokenExtras,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> OrmlTokenStorageInfo {
        let rawCurrencyId = try Data(hexString: extras.currencyIdScale)

        let decoder = try codingFactory.createDecoder(from: rawCurrencyId)
        let currencyId = try decoder.read(type: extras.currencyIdType)

        let moduleName: String

        let tokensTransfer = OrmlPallet.Transfer.codingPath(for: OrmlPallet.tokens)
        let transferAllPath: CallCodingPath

        if codingFactory.metadata.getCall(
            from: tokensTransfer.moduleName,
            with: tokensTransfer.callName
        ) != nil {
            moduleName = tokensTransfer.moduleName
            transferAllPath = OrmlPallet.TransferAll.codingPath(for: OrmlPallet.tokens)
        } else {
            moduleName = OrmlPallet.currencies
            transferAllPath = OrmlPallet.TransferAll.codingPath(for: OrmlPallet.currencies)
        }

        let existentialDeposit = BigUInt(extras.existentialDeposit) ?? 0

        let canTransferAll = codingFactory.metadata.getCall(
            from: transferAllPath.moduleName,
            with: transferAllPath.callName
        ) != nil

        return OrmlTokenStorageInfo(
            currencyId: currencyId,
            currencyData: rawCurrencyId,
            module: moduleName,
            existentialDeposit: existentialDeposit,
            canTransferAll: canTransferAll
        )
    }
}
