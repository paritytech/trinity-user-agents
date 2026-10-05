package io.paritytech.polkadotapp.feature_people_impl.data.personSetup.state

import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateFactory
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateStore
import io.paritytech.polkadotapp.common.data.worker.stateMachine.getParams
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_people_api.data.AliasContextProvider
import io.paritytech.polkadotapp.feature_people_api.data.SetAliasContext
import io.paritytech.polkadotapp.feature_people_api.data.repository.PersonIdRepository
import io.paritytech.polkadotapp.feature_people_impl.data.storage.PersonIdStorage
import javax.inject.Inject

class PersonSetupStateFactory @Inject constructor(
    private val personIdRepository: PersonIdRepository,
    private val personIdStorage: PersonIdStorage,
    private val accountRepository: AccountRepository,
    @SetAliasContext private val assignableContexts: Set<@JvmSuppressWildcards AliasContextProvider>
) : WorkerStateFactory<PersonSetupState> {
    override fun createState(
        stateId: String,
        store: WorkerStateStore<PersonSetupState>
    ): PersonSetupState? {
        return when (stateId) {
            SelfIncludeState.ID -> selfInclude()

            AwaitRingInclusionState.ID -> awaitRingInclusion()

            SetAliasState.ID -> setAliasState(store.getParams())

            SetPersonalIdAccountState.ID -> setPersonalIdAccount()

            AllDone.ID -> allDone()

            else -> null
        }
    }

    override fun createDefaultState(): PersonSetupState {
        return selfInclude()
    }

    fun selfInclude(): SelfIncludeState {
        return SelfIncludeState(stateFactory = this)
    }

    fun awaitRingInclusion(): AwaitRingInclusionState {
        return AwaitRingInclusionState(personIdStorage, stateFactory = this)
    }

    fun setAliasState(params: SetAliasState.Params): SetAliasState {
        return SetAliasState(
            accountRepository = accountRepository,
            personIdRepository = personIdRepository,
            stateFactory = this,
            assignableContexts = assignableContexts,
            params = params
        )
    }

    fun setPersonalIdAccount(): SetPersonalIdAccountState {
        return SetPersonalIdAccountState(stateFactory = this)
    }

    fun allDone(): AllDone {
        return AllDone()
    }
}
