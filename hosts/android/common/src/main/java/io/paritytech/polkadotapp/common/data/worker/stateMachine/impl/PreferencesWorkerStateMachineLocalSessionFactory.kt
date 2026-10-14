package io.paritytech.polkadotapp.common.data.worker.stateMachine.impl

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.data.storage.preferences.Editor
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.edit
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateFactory
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineLocalSession
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import java.lang.reflect.Type
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class PrefsWorkerStateMachineLocalSessionFactory @Inject constructor(
    private val preferences: Preferences,
    private val gson: Gson,
) {
    fun <S : WorkerStateMachineState<S, *>> create(
        uniquePrefix: String,
        stateFactory: WorkerStateFactory<S>
    ): WorkerStateMachineLocalSession<S> {
        return PreferencesWorkerStateMachineLocalSession(
            preferences = preferences,
            gson = gson,
            stateFactory = stateFactory,
            uniquePrefix = uniquePrefix
        )
    }
}

private class PreferencesWorkerStateMachineLocalSession<S : WorkerStateMachineState<S, *>>(
    private val preferences: Preferences,
    private val gson: Gson,
    private val stateFactory: WorkerStateFactory<S>,
    uniquePrefix: String,
) : WorkerStateMachineLocalSession<S> {
    companion object {
        private const val CURRENT_STATE_KEY = "StateId"

        private const val CURRENT_STATE_PARAMS_KEY = "StateParams"

        private const val FAILURE_CAUSE_ID_KEY = "FailureCauseId"

        private const val FAILURE_CAUSE_PARAMS_KEY = "FailureCauseParams"
    }

    private val currentStateIdKey = uniquePrefix + CURRENT_STATE_KEY
    private val currentStateParamsKey = uniquePrefix + CURRENT_STATE_PARAMS_KEY
    private val retryStateIdKey = uniquePrefix + FAILURE_CAUSE_ID_KEY
    private val retryStateParamsKey = uniquePrefix + FAILURE_CAUSE_PARAMS_KEY

    override suspend fun getCurrentState(): S? {
        val stateId = getStateId(currentStateIdKey) ?: return null

        val store = StateStore(currentStateParamsKey)
        return stateFactory.createState(stateId, store)
    }

    override suspend fun setCurrentState(state: S) = preferences.edit {
        saveStateInTransaction(currentStateIdKey, currentStateParamsKey, state)
    }

    override fun currentStateFlow(): Flow<S?> {
        return preferences.stringFlow(currentStateIdKey)
            .map { getCurrentState() }
    }

    override suspend fun resetState() {
        preferences.edit {
            remove(currentStateIdKey)
            remove(currentStateParamsKey)
            remove(retryStateIdKey)
            remove(retryStateParamsKey)
        }
    }

    context(editor: Editor)
    private fun saveStateInTransaction(idKey: String, paramKey: String, state: S) {
        saveStateId(idKey, state.id)

        val params = (state as? WorkerStateMachineState.WithParams<*>)?.params
        saveStateParams(paramKey, params)

        if (state is WorkerStateMachineState.UnrecoverableFailureState<*>) {
            saveStateInTransaction(retryStateIdKey, retryStateParamsKey, state)
        }
    }

    private fun getStateId(idKey: String): String? = preferences.getString(idKey)

    private fun getStateIdOrThrow(idKey: String): String = requireNotNull(getStateId(idKey)) {
        "No state key found: $idKey"
    }

    context(editor: Editor)
    private fun saveStateId(idKey: String, stateId: String) = editor.putString(idKey, stateId)

    context(editor: Editor)
    private fun saveStateParams(idKey: String, params: Any?) {
        val serializedParams = gson.toJson(params)
        editor.putString(idKey, serializedParams)
    }

    private inner class StateStore(
        private val paramsKey: String,
    ) : WorkerStateStore<S> {
        override fun getRetryState(): S {
            val retryStateId = getStateIdOrThrow(retryStateIdKey)
            val retryStateStore = StateStore(retryStateParamsKey)

            val retryState = stateFactory.createState(retryStateId, retryStateStore)
            return requireNotNull(retryState) {
                "Retry state was not found"
            }
        }

        override fun <P> getParams(type: Type): P {
            val raw = requireNotNull(preferences.getString(paramsKey))
            return gson.fromJson(raw, type)
        }
    }
}
