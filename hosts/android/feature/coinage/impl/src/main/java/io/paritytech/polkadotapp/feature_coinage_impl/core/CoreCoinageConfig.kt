package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.database.dao.CoinDao
import io.paritytech.polkadotapp.database.dao.CoinageInstallationDao
import io.paritytech.polkadotapp.database.dao.RecyclerVoucherDao
import io.paritytech.polkadotapp.database.model.CoinageInstallationLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageHandOverPayments
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.CoreCoinageSetup
import uniffi.truapi.AppInstallation
import uniffi.truapi.CoinageConfig
import uniffi.truapi.Handover
import uniffi.truapi.IncomingPayment
import uniffi.truapi.OutstandingPayment
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.ExperimentalTime

/**
 * How the core's coinage layer starts: with the installation registry from the remote config, and with the wallet
 * the app's own coinage engine kept, which the core takes over the first time it starts.
 */
@Singleton
class CoreCoinageConfig @Inject constructor(
    private val installationDao: CoinageInstallationDao,
    private val coinDao: CoinDao,
    private val voucherDao: RecyclerVoucherDao,
    private val handOverPayments: CoinageHandOverPayments,
    private val dataStoreConfigProvider: AccountDataStoreConfigProvider,
) : CoreCoinageSetup {
    override suspend fun config(): CoinageConfig {
        val registry = registryContract()
        val handOver = handOver()
        coinageLogI("Core coinage config: registry=${registry != null} handOverInstallations=${handOver?.installations?.size ?: 0}")
        return CoinageConfig(handOver = handOver, registryContract = registry?.value?.toHexString(withPrefix = true))
    }

    /** The installation registry contract, from the last synced remote config, or a sync when none is cached. */
    suspend fun registryContract(): DataByteArray? =
        dataStoreConfigProvider.lastKnownContractAddress().getOrNull() ?: dataStoreConfigProvider.contractAddress().getOrNull()

    /** None when the app never ran its own engine on this seed. */
    private suspend fun handOver(): Handover? {
        val current = installationDao.getCurrent() ?: return null
        val installations = (listOf(current) + installationDao.getPrevious()).map { it.toAppInstallation() }
        val payments = handOverPayments.payments()
        return Handover(
            installations = installations,
            outstanding = payments.outgoing.map { payment ->
                OutstandingPayment(
                    id = payment.exportId.value.value,
                    accounts = payment.coinKeys.map { it.deriveKeypair().publicKey },
                )
            },
            incoming = payments.incoming.map { payment ->
                @OptIn(ExperimentalTime::class)
                IncomingPayment(
                    id = CoinageIds.of(payment.claimGroupId),
                    claimBy = payment.claimBy.epochSeconds.toULong(),
                    coins = payment.coinKeys.map { it.value },
                )
            },
        )
    }

    /** The marks cover every index the engine allocated, and every index its own recovery scan already passed. */
    private suspend fun CoinageInstallationLocal.toAppInstallation() = AppInstallation(
        id = installationId,
        nextCoin = maxOf(coinDao.getMaxDerivationIndex(installationId)?.plus(1) ?: 0, coinScanNextIndex).toUInt(),
        nextEntry = maxOf(voucherDao.getMaxRingVrfKeyIndex(installationId)?.plus(1) ?: 0, voucherScanNextIndex).toUInt(),
    )
}
