package io.paritytech.polkadotapp.feature_people_api.domain

interface PeopleCheckMemberInRingUseCase {
    suspend fun awaitIncluded(peopleCollection: PeopleCollection): Result<Unit>

    suspend fun checkIncludes(peopleCollection: PeopleCollection): Result<Boolean>
}
