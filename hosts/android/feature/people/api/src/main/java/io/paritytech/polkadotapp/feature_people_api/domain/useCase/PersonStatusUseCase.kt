package io.paritytech.polkadotapp.feature_people_api.domain.useCase

import io.paritytech.polkadotapp.feature_people_api.domain.models.PersonhoodStatus
import io.paritytech.polkadotapp.feature_people_api.domain.models.isActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first

interface PersonStatusUseCase {
    fun personhoodStatusFlow(): Flow<PersonhoodStatus>
}

suspend fun PersonStatusUseCase.isPersonhoodActive(): Boolean {
    return personhoodStatusFlow().first().isActive()
}
