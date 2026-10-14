package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.database.model.chain.ChainRuntimeInfoLocal

interface RuntimeVersionsRepository {
    suspend fun getAllRuntimeVersions(): List<RuntimeVersion>
}

internal class DbRuntimeVersionsRepository(
    private val chainDao: ChainDao,
) : RuntimeVersionsRepository {
    override suspend fun getAllRuntimeVersions(): List<RuntimeVersion> {
        return chainDao.allRuntimeInfos().map(::mapRuntimeInfoLocalToRuntimeVersion)
    }

    private fun mapRuntimeInfoLocalToRuntimeVersion(runtimeInfoLocal: ChainRuntimeInfoLocal): RuntimeVersion {
        return RuntimeVersion(
            chainId = runtimeInfoLocal.chainId,
            specVersion = runtimeInfoLocal.syncedVersion
        )
    }
}
