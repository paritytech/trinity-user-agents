package io.paritytech.polkadotapp.chains.util

import io.novasama.substrate_sdk_android.extensions.asEthereumAccountId
import io.novasama.substrate_sdk_android.extensions.asEthereumAddress
import io.novasama.substrate_sdk_android.extensions.asEthereumPublicKey
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.isValid
import io.novasama.substrate_sdk_android.extensions.toAccountId
import io.novasama.substrate_sdk_android.extensions.toAddress
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.addressPrefix
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAddress
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ExplorerTemplateExtractor
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.TokenSymbol
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.TypesUsage
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.emptyEthereumAccountId
import io.paritytech.polkadotapp.common.utils.emptySubstrateAccountId
import io.paritytech.polkadotapp.common.utils.findIsInstanceOrNull
import io.paritytech.polkadotapp.common.utils.formatNamed
import io.paritytech.polkadotapp.common.utils.substrateAccountId
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import java.math.BigDecimal

val Chain.typesUsage: TypesUsage
    get() =
        when {
            types == null -> TypesUsage.NONE
            !types.overridesCommon && types.url != null -> TypesUsage.BOTH
            !types.overridesCommon && types.url == null -> TypesUsage.BASE
            else -> TypesUsage.OWN
        }

val TypesUsage.requiresBaseTypes: Boolean
    get() = this == TypesUsage.BASE || this == TypesUsage.BOTH

val Chain.utilityAsset
    get() = assets.first(Chain.Asset::isUtilityAsset)

val Chain.isSubstrateBased
    get() = !isEthereumBased

val Chain.commissionAsset
    get() = utilityAsset

val Chain.ConnectionState.isFullSync: Boolean
    get() = this == Chain.ConnectionState.FULL_SYNC

val Chain.ConnectionState.isDisabled: Boolean
    get() = this == Chain.ConnectionState.DISABLED

val Chain.ConnectionState.level: Int
    get() =
        when (this) {
            Chain.ConnectionState.FULL_SYNC -> 2
            Chain.ConnectionState.LIGHT_SYNC -> 1
            Chain.ConnectionState.DISABLED -> 0
        }

inline fun <reified T : Chain.ExternalApi> Chain.externalApi(): T? {
    return externalApis.findIsInstanceOrNull<T>()
}

const val UTILITY_ASSET_ID = 0

val Chain.Asset.isUtilityAsset: Boolean
    get() = id == UTILITY_ASSET_ID

inline val FullChainAssetId.isUtility: Boolean
    get() = assetId == UTILITY_ASSET_ID

inline val ChainId.utilityAssetId: FullChainAssetId
    get() = FullChainAssetId(this, UTILITY_ASSET_ID)

val Chain.Node.isWs: Boolean
    get() = connectionType == Chain.Node.ConnectionType.WS

val Chain.Node.isHttp: Boolean
    get() = connectionType == Chain.Node.ConnectionType.HTTP

fun Chain.Nodes.wssNodes(): List<Chain.Node> {
    return nodes.filter { it.isWs }
}

fun Chain.Nodes.httpNodes(): Chain.Nodes {
    return copy(nodes = nodes.filter { it.isHttp })
}

val Chain.Asset.disabled: Boolean
    get() = !enabled

fun Chain.addressOf(accountId: AccountId): String {
    return if (isEthereumBased) {
        accountId.value.toEthereumAddress()
    } else {
        accountId.value.toAddress(addressPrefix.toShort())
    }
}

fun ByteArray.toEthereumAddress(): String {
    return asEthereumAccountId().toAddress(withChecksum = true).value
}

fun Chain.accountIdOf(address: String): AccountId {
    val rawAccountId = if (isEthereumBased) {
        address.asEthereumAddress().toAccountId().value
    } else {
        address.toAccountId()
    }

    return rawAccountId.intoAccountId()
}

fun Chain.emptyAccountId() =
    if (isEthereumBased) {
        emptyEthereumAccountId()
    } else {
        emptySubstrateAccountId()
    }

fun Chain.accountIdOf(publicKey: ByteArray): AccountId {
    return if (isEthereumBased) {
        publicKey.asEthereumPublicKey().toAccountId().value.intoAccountId()
    } else {
        publicKey.substrateAccountId()
    }
}

fun Chain.isValidAddress(address: String): Boolean {
    return runCatching {
        if (isEthereumBased) {
            address.asEthereumAddress().isValid()
        } else {
            address.toAccountId() // verify supplied address can be converted to account id

            address.addressPrefix() == addressPrefix.toShort()
        }
    }.getOrDefault(false)
}

fun Chain.isValidEvmAddress(address: String): Boolean {
    return runCatching {
        if (isEthereumBased) {
            address.asEthereumAddress().isValid()
        } else {
            false
        }
    }.getOrDefault(false)
}

val Chain.isParachain
    get() = parentId != null

fun Chain.availableExplorersFor(field: ExplorerTemplateExtractor) =
    explorers.filter { field(it) != null }

fun Chain.Explorer.accountUrlOf(address: String): String {
    return format(Chain.Explorer::account, "address", address)
}

fun Chain.Explorer.extrinsicUrlOf(extrinsicHash: String): String {
    return format(Chain.Explorer::extrinsic, "hash", extrinsicHash)
}

fun Chain.Explorer.eventUrlOf(eventId: String): String {
    return format(Chain.Explorer::event, "event", eventId)
}

fun Chain.Asset.requireAssets(): Type.Assets {
    require(type is Type.Assets)

    return type
}

fun Chain.Asset.requireOrml(): Type.Orml {
    require(type is Type.Orml)

    return type
}

fun Chain.Asset.amountFromPlanks(planks: Balance): BigDecimal {
    return planks.amountFromPlanks(precision)
}

fun Chain.Asset.planksFromAmount(amount: BigDecimal): Balance {
    return amount.planksFromAmount(precision)
}

fun Chain.Asset.normalizeSymbol(): String {
    return normalizeTokenSymbol(this.symbol)
}

private const val XC_PREFIX = "xc"

fun normalizeTokenSymbol(symbol: String): String {
    return symbol.removePrefix(XC_PREFIX)
}

private inline fun Chain.Explorer.format(
    templateExtractor: ExplorerTemplateExtractor,
    argumentName: String,
    argumentValue: String,
): String {
    val template = templateExtractor(this)
        ?: throw Exception("Cannot find template in the chain explorer: $name")

    return template.formatNamed(argumentName to argumentValue)
}

object ChainIds {
    const val POLKADOT = "91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3"
    const val POLKADOT_HYDRATION = "afdc188f45c71dacbaa0b62e16a91f726c7b8699a9748cdf715459de6b7f366d"
    const val KUSAMA = "b0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe"
    const val KUSAMA_ASSET_HUB = "48239ef607d7928874027a43a67689209727dfb3d3dc5e5b03a39bdc2eda771a"

    const val NIGHTLY_PEOPLE = "nightly-people"
    const val NIGHTLY_BULLET_IN = "nightly-bulletin"
    const val NIGHTLY_ASSET_HUB = "nightly-ah"

    const val PREVIEWNET_PEOPLE = "preview-people"
    const val PREVIEWNET_BULLET_IN = "preview-bulletin"
    const val PREVIEWNET_ASSET_HUB = "preview-ah"

    const val RELEASE_PEOPLE = "release-people"
    const val RELEASE_ASSET_HUB = "release-ah"
    const val RELEASE_BULLETIN = "release-bulletin"

    fun createEvmChainId(evmChainId: Int): ChainId {
        return "${EIP_155_PREFIX}:$evmChainId"
    }
}

object ChainGeneses {
    val POLKADOT = "91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3".asGenesisHash()
    val KUSAMA = "b0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe".asGenesisHash()

    val POLKADOT_ASSET_HUB = "68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f".asGenesisHash()
    val KUSAMA_ASSET_HUB = "48239ef607d7928874027a43a67689209727dfb3d3dc5e5b03a39bdc2eda771a".asGenesisHash()
}

private const val EIP_155_PREFIX = "eip155"

private fun String.asGenesisHash(): GenesisHash {
    return fromHex().toDataByteArray()
}

fun Chain.evmChainIdOrNull(): Int? {
    return if (id.startsWith(EIP_155_PREFIX)) {
        id.removePrefix("${EIP_155_PREFIX}:")
            .toIntOrNull()
    } else {
        null
    }
}

val Chain.Companion.Ids
    get() = ChainIds

val Chain.Companion.Geneses
    get() = ChainGeneses

val Chain.Asset.fullId: FullChainAssetId
    get() = FullChainAssetId(chainId, id)

fun Chain.enabledAssets(): List<Chain.Asset> = assets.filter { it.enabled }

fun Chain.disabledAssets(): List<Chain.Asset> = assets.filterNot { it.enabled }

fun Chain.findAssetByNormalizedSymbol(symbol: TokenSymbol): Chain.Asset? {
    val normalized = normalizeTokenSymbol(symbol)
    return assets.find { it.normalizeSymbol() == normalized }
}

val Chain.Asset.databaseId: FullChainAssetIdLocal
    get() = FullChainAssetIdLocal(chainId, id)

fun FullChainAssetId.toLocal(): FullChainAssetIdLocal {
    return FullChainAssetIdLocal(chainId, assetId)
}

fun FullChainAssetIdLocal.toDomain(): FullChainAssetId {
    return FullChainAssetId(chainId, assetId)
}
