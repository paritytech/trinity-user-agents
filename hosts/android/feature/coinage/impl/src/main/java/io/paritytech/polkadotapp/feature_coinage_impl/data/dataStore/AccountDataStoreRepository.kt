package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.InstallationRegistrationTarget
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import timber.log.Timber
import javax.inject.Inject

class InstallationRegistrationCall(
    val account: DataStoreAccount,
    val contract: EvmAccountId,
    val input: DataByteArray,
)

interface AccountDataStoreRepository {
    // Records this seed cannot open are skipped rather than failing the read: the list belongs to the account, and
    // nothing guarantees every entry in it was written by this app.
    suspend fun fetchRegisteredInstallations(contract: EvmAccountId, at: BlockHash?): Result<Set<CoinageInstallationId>>

    suspend fun registrationCall(target: InstallationRegistrationTarget): Result<InstallationRegistrationCall>
}

class RealAccountDataStoreRepository @Inject constructor(
    private val knownChains: KnownChains,
    private val accountKeys: DataStoreAccountKeys,
    private val recordCipher: InstallationRecordCipher,
    private val reviveContractApi: ReviveContractApi,
) : AccountDataStoreRepository {
    override suspend fun fetchRegisteredInstallations(contract: EvmAccountId, at: BlockHash?): Result<Set<CoinageInstallationId>> {
        return accountKeys.get().flatMap { account ->
            val input = AccountDataStoreContractCoder.encodeGetInstallations(account.evmAccountId)
            reviveContractApi.callReadOnly(knownChains.assetHub, contract, input, at)
                .mapCatching { AccountDataStoreContractCoder.decodeGetInstallations(it) }
                .map { records -> records.openWith(account) }
        }
    }

    override suspend fun registrationCall(target: InstallationRegistrationTarget): Result<InstallationRegistrationCall> {
        return accountKeys.get().map { account ->
            val record = recordCipher.seal(target.installation, account.encryptionKey)

            InstallationRegistrationCall(
                account = account,
                contract = target.contract,
                input = AccountDataStoreContractCoder.encodeRegisterInstallation(record),
            )
        }
    }

    private fun List<DataByteArray>.openWith(account: DataStoreAccount): Set<CoinageInstallationId> {
        val opened = mapNotNull { recordCipher.open(it, account.encryptionKey) }.toSet()

        if (opened.size < size) {
            Timber.w("Skipped ${size - opened.size} of $size data store records this seed could not open or that repeat")
        }

        return opened
    }
}
