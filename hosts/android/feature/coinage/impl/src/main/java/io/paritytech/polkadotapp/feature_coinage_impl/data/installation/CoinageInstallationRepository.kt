package io.paritytech.polkadotapp.feature_coinage_impl.data.installation

import io.paritytech.polkadotapp.database.dao.CoinageInstallationDao
import io.paritytech.polkadotapp.database.model.CoinageInstallationLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import javax.inject.Inject

data class PreviousInstallation(
    val id: CoinageInstallationId,
    val coinScanNextIndex: Int,
    val voucherScanNextIndex: Int,
    val initialScanCompleted: Boolean,
)

interface CoinageInstallationRepository {
    suspend fun getOrCreateCurrent(): CoinageInstallationId

    suspend fun addPrevious(installations: Collection<CoinageInstallationId>)

    suspend fun getPrevious(): List<PreviousInstallation>

    suspend fun updateCoinScanNextIndex(installation: CoinageInstallationId, nextIndex: Int)

    suspend fun updateVoucherScanNextIndex(installation: CoinageInstallationId, nextIndex: Int)

    suspend fun markInitialScanCompleted(installation: CoinageInstallationId)
}

class RealCoinageInstallationRepository @Inject constructor(
    private val installationDao: CoinageInstallationDao,
) : CoinageInstallationRepository {
    override suspend fun getOrCreateCurrent(): CoinageInstallationId {
        return installationDao.getOrCreateCurrent { CoinageInstallationId.random().value.value }
            .installationId
            .toCoinageInstallationId()
    }

    override suspend fun addPrevious(installations: Collection<CoinageInstallationId>) {
        val current = getOrCreateCurrent()
        val previous = installations.filter { it != current }.map { it.toPreviousLocal() }

        installationDao.insertIfAbsent(previous)
    }

    override suspend fun getPrevious(): List<PreviousInstallation> {
        return installationDao.getPrevious().map { it.toPreviousInstallation() }
    }

    override suspend fun updateCoinScanNextIndex(installation: CoinageInstallationId, nextIndex: Int) {
        installationDao.updateCoinScanNextIndex(installation.value.value, nextIndex)
    }

    override suspend fun updateVoucherScanNextIndex(installation: CoinageInstallationId, nextIndex: Int) {
        installationDao.updateVoucherScanNextIndex(installation.value.value, nextIndex)
    }

    override suspend fun markInitialScanCompleted(installation: CoinageInstallationId) {
        installationDao.markInitialScanCompleted(installation.value.value)
    }

    private fun CoinageInstallationId.toPreviousLocal() = CoinageInstallationLocal(
        installationId = value.value,
        isCurrent = false,
        coinScanNextIndex = 0,
        voucherScanNextIndex = 0,
        initialScanCompleted = false,
    )

    private fun CoinageInstallationLocal.toPreviousInstallation() = PreviousInstallation(
        id = installationId.toCoinageInstallationId(),
        coinScanNextIndex = coinScanNextIndex,
        voucherScanNextIndex = voucherScanNextIndex,
        initialScanCompleted = initialScanCompleted,
    )
}
