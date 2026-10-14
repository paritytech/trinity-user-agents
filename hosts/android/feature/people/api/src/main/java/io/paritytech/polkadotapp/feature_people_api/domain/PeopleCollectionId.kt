package io.paritytech.polkadotapp.feature_people_api.domain

import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId

val RingCollectionId.Companion.PEOPLE: RingCollectionId
    get() = paddedString("pop:polkadot.network/people")

val RingCollectionId.Companion.PEOPLE_LITE: RingCollectionId
    get() = paddedString("pop:polkadot.network/people-lite")

fun PeopleCollection.toRingCollectionId(): RingCollectionId = when (this) {
    PeopleCollection.People -> RingCollectionId.PEOPLE
    PeopleCollection.LitePeople -> RingCollectionId.PEOPLE_LITE
}
