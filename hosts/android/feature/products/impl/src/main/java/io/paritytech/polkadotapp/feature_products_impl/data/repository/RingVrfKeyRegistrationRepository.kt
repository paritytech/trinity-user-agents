package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.database.dao.RingVrfKeyRegistrationDao
import io.paritytech.polkadotapp.database.model.RingVrfKeyRegistrationLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.mappers.toDomain
import io.paritytech.polkadotapp.feature_products_impl.data.mappers.toLocal
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistration
import javax.inject.Inject

interface RingVrfKeyRegistrationRepository {
    suspend fun save(registration: RingVrfKeyRegistration)

    suspend fun getByOwner(owner: ProductId): List<RingVrfKeyRegistration>

    suspend fun getByHandle(handle: ProductAccountId): List<RingVrfKeyRegistration>
}

class RealRingVrfKeyRegistrationRepository @Inject constructor(
    private val dao: RingVrfKeyRegistrationDao,
) : RingVrfKeyRegistrationRepository {
    override suspend fun save(registration: RingVrfKeyRegistration) {
        dao.insert(registration.toLocal())
    }

    override suspend fun getByOwner(owner: ProductId): List<RingVrfKeyRegistration> {
        return dao.getByOwner(owner.value).toDomain()
    }

    override suspend fun getByHandle(handle: ProductAccountId): List<RingVrfKeyRegistration> {
        return dao.getByHandle(handle.productId, handle.index.bytes.value).toDomain()
    }

    // A row whose ring or index no longer decodes is dropped rather than failing the whole listing:
    // one unreadable registration should not hide every other key the product owns.
    private fun List<RingVrfKeyRegistrationLocal>.toDomain(): List<RingVrfKeyRegistration> =
        mapNotNull { it.toDomain().logFailure("RingVrfKeyRegistrationRepository").getOrNull() }
}
