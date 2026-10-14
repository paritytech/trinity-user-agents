package io.paritytech.polkadotapp.feature_people_impl.domain.useCase

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.PersonStatusUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.isPersonhoodActive
import javax.inject.Inject

class RealActivePeopleCollectionUseCase @Inject constructor(
    private val personStatusUseCase: PersonStatusUseCase
) : ActivePeopleCollectionUseCase {
    override suspend fun getActivePeopleCollection(): PeopleCollection {
        return if (personStatusUseCase.isPersonhoodActive()) {
            PeopleCollection.People
        } else {
            PeopleCollection.LitePeople
        }
    }

    override suspend fun getAvailableCollections(): List<PeopleCollection> = buildList {
        if (personStatusUseCase.isPersonhoodActive()) {
            add(PeopleCollection.People)
        }
        add(PeopleCollection.LitePeople)
    }
}
