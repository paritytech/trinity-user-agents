package io.paritytech.polkadotapp.feature_people_impl.domain.useCase

import io.paritytech.polkadotapp.feature_people_api.domain.models.PersonhoodStatus
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.PersonStatusUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

class DisabledPersonStatusUseCase @Inject constructor() : PersonStatusUseCase {
    override fun personhoodStatusFlow(): Flow<PersonhoodStatus> {
        return flowOf(PersonhoodStatus.NotPerson)
    }
}
