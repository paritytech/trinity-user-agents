package io.paritytech.polkadotapp.feature_sso_impl.data

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.SsoSessionDao
import io.paritytech.polkadotapp.feature_sso_api.domain.OwnDevicesJournal
import javax.inject.Inject

class RealOwnDevicesJournal @Inject constructor(
    private val ssoSessionDao: SsoSessionDao,
) : OwnDevicesJournal {
    override suspend fun getOutgoingUpdateTime(deviceStatementAccountId: AccountId): Long? {
        return ssoSessionDao.getOutgoingUpdateTime(deviceStatementAccountId.value)
    }

    override suspend fun updateOutgoingUpdateTime(deviceStatementAccountId: AccountId, timePoint: Long) {
        ssoSessionDao.updateOutgoingUpdateTime(deviceStatementAccountId.value, timePoint)
    }

    override suspend fun getLastSyncOfferId(deviceStatementAccountId: AccountId): String? {
        return ssoSessionDao.getLastSyncOfferId(deviceStatementAccountId.value)
    }

    override suspend fun saveLastSyncOfferId(deviceStatementAccountId: AccountId, offerId: String) {
        ssoSessionDao.updateLastSyncOfferId(deviceStatementAccountId.value, offerId)
    }
}
