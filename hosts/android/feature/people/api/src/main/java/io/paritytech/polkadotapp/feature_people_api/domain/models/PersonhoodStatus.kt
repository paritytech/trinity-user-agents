package io.paritytech.polkadotapp.feature_people_api.domain.models

sealed class PersonhoodStatus {
    object NotPerson : PersonhoodStatus()

    object Suspended : PersonhoodStatus()

    object Onboarding : PersonhoodStatus()

    object Active : PersonhoodStatus()
}

fun PersonhoodStatus.isActive(): Boolean {
    return this is PersonhoodStatus.Active
}
