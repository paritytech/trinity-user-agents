package io.paritytech.polkadotapp.feature_people_impl.data.personSetup.state

import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure

class SetPersonalIdAccountState(
    private val stateFactory: PersonSetupStateFactory,
) : PersonSetupNonTerminalState() {
    companion object {
        val ID = "SetPersonalIdAccount"
    }

    override val id = ID

    context(transition: PersonSetupState.Transition)
    override suspend fun performNonTerminalTransition(): Result<PersonSetupState> {
        val personalAccountId = transition.dataSource.candidateAccount.accountIdIn(transition.dataSource.peopleChain)

        return transition.dataSource.setPersonalIdAccount(personalAccountId)
            .flattenExecutionFailure()
            .map { stateFactory.allDone() }
    }
}
