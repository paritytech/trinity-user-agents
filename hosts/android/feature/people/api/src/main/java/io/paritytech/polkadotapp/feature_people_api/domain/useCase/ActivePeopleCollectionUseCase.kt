package io.paritytech.polkadotapp.feature_people_api.domain.useCase

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection

interface ActivePeopleCollectionUseCase {
    suspend fun getActivePeopleCollection(): PeopleCollection

    /**
     * Every collection whose aliases this device can currently produce: always
     * [PeopleCollection.LitePeople] (backed by the wallet account), plus
     * [PeopleCollection.People] once personhood is active (candidate account present).
     */
    suspend fun getAvailableCollections(): List<PeopleCollection>
}
