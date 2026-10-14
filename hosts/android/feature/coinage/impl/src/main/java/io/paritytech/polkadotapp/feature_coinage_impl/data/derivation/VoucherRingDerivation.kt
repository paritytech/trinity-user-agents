package io.paritytech.polkadotapp.feature_coinage_impl.data.derivation

import io.novasama.substrate_sdk_android.encrypt.junction.JunctionType
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import javax.inject.Inject

interface VoucherRingDerivation {
    suspend fun deriveBandersnatch(derivationIndex: CoinageKeyIndex): BandersnatchEntropy

    suspend fun deriveBandersnatchBatch(derivationIndices: List<CoinageKeyIndex>): List<BandersnatchEntropy>

    /**
     * This and [aliasOf] are interface members rather than top-level extensions so we can fake them in tests - JVM tests cant access Bandersnatch
     * Mocking via mockStatic is not an option - its too slow for the fuzzer
     */
    suspend fun memberKeyOf(derivationIndex: CoinageKeyIndex): BandersnatchPublicKey

    suspend fun aliasOf(derivationIndex: CoinageKeyIndex, context: BandersnatchContext): BandersnatchAlias
}

class RealVoucherRingDerivation @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
) : VoucherRingDerivation {
    override suspend fun deriveBandersnatch(derivationIndex: CoinageKeyIndex): BandersnatchEntropy {
        val path = ringVrfDerivationBase(derivationIndex.installation) + itemDerivationSegment(derivationIndex.item)

        return deriveBandersnatch(rootEntropy(), path)
    }

    override suspend fun memberKeyOf(derivationIndex: CoinageKeyIndex): BandersnatchPublicKey =
        deriveBandersnatch(derivationIndex).memberKey()

    override suspend fun aliasOf(derivationIndex: CoinageKeyIndex, context: BandersnatchContext): BandersnatchAlias =
        deriveBandersnatch(derivationIndex).aliasInContext(context)

    override suspend fun deriveBandersnatchBatch(derivationIndices: List<CoinageKeyIndex>): List<BandersnatchEntropy> {
        val entropy = rootEntropy()

        return derivationIndices.deriveGroupedByInstallation(
            base = { installation -> deriveBandersnatch(entropy, ringVrfDerivationBase(installation)) },
            child = { base, item -> deriveBandersnatch(base.value, itemDerivationSegment(item)) }
        )
    }

    private suspend fun rootEntropy(): ByteArray {
        val accountId = accountRepository.getWalletAccount().id
        return accountSecretsStorage.requireMetaAccountPassphrase(accountId).entropy
    }

    private fun deriveBandersnatch(entropy: ByteArray, derivationPath: String): BandersnatchEntropy {
        val decodedPath = SubstrateJunctionDecoder.decode(derivationPath)
        val junctions = decodedPath.junctions

        val derivedEntropy = junctions.fold(entropy) { currentEntropy, junction ->
            require(junction.type == JunctionType.HARD) {
                "Ring-VRF derivation only supports HARD junctions, but found ${junction.type}"
            }

            currentEntropy.blake2b256(junction.chaincode)
        }

        return BandersnatchEntropy(derivedEntropy)
    }

    // Unlike coinage keys, the item junction is hard - ring-vrf entropy derivation is a blake2b chaincode chain with no soft variant
    private fun itemDerivationSegment(item: Int): String {
        return "//$item"
    }

    private fun ringVrfDerivationBase(installation: CoinageInstallationId): String {
        return "//coinage-ring-vrf//${CoinageDerivationDefaults.COINAGE_MAIN_PURSE_INDEX}//${installation.asPageSegment()}"
    }
}

suspend fun VoucherRingDerivation.getDerivedMemberKey(derivationIndex: CoinageKeyIndex) =
    deriveBandersnatch(derivationIndex).memberKey()

suspend fun VoucherRingDerivation.getDerivedMemberKeys(derivationIndices: List<CoinageKeyIndex>) =
    deriveBandersnatchBatch(derivationIndices).map { it.memberKey() }

suspend fun VoucherRingDerivation.deriveBandersnatchForVouchers(vouchers: List<RecyclerVoucher>) = vouchers.map {
    deriveBandersnatch(it.ringVrfKeyIndex)
}
