package io.paritytech.polkadotapp.feature_people_api.data.updaters

import io.paritytech.polkadotapp.chains.network.updaters.Updater

class PeopleUpdaters(val peopleChainUpdaters: List<Updater<*>>)
