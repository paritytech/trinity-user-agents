package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.database.dao.BrowserTabDao
import io.paritytech.polkadotapp.database.model.BrowserTabLocal
import javax.inject.Inject

data class PersistedTab(
    val id: Long,
    val url: String,
    val title: String,
    val position: Int,
    val lastActive: Long,
)

interface BrowserTabRepository {
    suspend fun loadAll(): List<PersistedTab>
    suspend fun save(tab: PersistedTab)
    suspend fun delete(id: Long)
}

class RealBrowserTabRepository @Inject constructor(
    private val dao: BrowserTabDao,
) : BrowserTabRepository {
    override suspend fun loadAll(): List<PersistedTab> =
        dao.getAll().map { PersistedTab(it.id, it.url, it.title, it.position, it.lastActive) }

    override suspend fun save(tab: PersistedTab) {
        dao.upsert(BrowserTabLocal(tab.id, tab.url, tab.title, tab.position, tab.lastActive))
    }

    override suspend fun delete(id: Long) = dao.deleteById(id)
}
