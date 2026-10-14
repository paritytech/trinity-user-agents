package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.InstallationRegistrationCall
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.calls.call
import io.paritytech.polkadotapp.feature_revive_api.calls.revive
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SignedTransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import java.math.BigDecimal
import java.math.BigInteger
import javax.inject.Inject

interface InstallationRegistrationSubmitter {
    suspend fun submitAttempt(target: InstallationRegistrationTarget): Result<DurableTxId>
}

class DataStoreAccountUnmappedError : Exception("The data store account has no Revive address mapping")

class RealInstallationRegistrationSubmitter @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val dataStoreRepository: AccountDataStoreRepository,
    private val reviveContractApi: ReviveContractApi,
    private val pgasProvisioner: DataStorePgasProvisioner,
    private val extrinsicService: ExtrinsicService,
    private val durableTransactionService: DurableTransactionService,
) : InstallationRegistrationSubmitter {
    override suspend fun submitAttempt(target: InstallationRegistrationTarget): Result<DurableTxId> {
        val chain = chainRegistry.getChain(knownChains.assetHub)

        return dataStoreRepository.registrationCall(target).flatMap { call ->
            prepareAccount(call)
                .flatMap { estimateCost(chain, call) }
                .flatMap { cost -> pgasProvisioner.ensureCovers(call.account.accountId, cost.required).map { cost } }
                .flatMap { cost -> register(chain, call, cost, target) }
        }
    }

    private suspend fun prepareAccount(call: InstallationRegistrationCall): Result<Unit> {
        return pgasProvisioner.ensureFunded(call.account.accountId).flatMap { requireMapped(call) }
    }

    // A dry-run maps the origin for the length of the simulation, so only storage tells whether a real call would pass.
    private suspend fun requireMapped(call: InstallationRegistrationCall): Result<Unit> {
        return reviveContractApi.isAccountMapped(knownChains.assetHub, call.account.accountId).flatMap { mapped ->
            if (mapped) Result.success(Unit) else Result.failure(DataStoreAccountUnmappedError())
        }
    }

    private suspend fun estimateCost(chain: Chain, call: InstallationRegistrationCall): Result<RegistrationCost> {
        return reviveContractApi.dryRun(chain.id, call.account.accountId, call.contract, call.input).flatMap { dryRun ->
            val formExtrinsic = call.formExtrinsic(dryRun.weightRequired.withMargin(), dryRun.storageDeposit.withMargin())

            extrinsicService.estimateFee(chain, call.origin(), formExtrinsic = formExtrinsic).map { fee ->
                val required = dryRun.storageDeposit.withMargin().intoBalance() + fee.amount
                coinageLogI("Installation registration: dry-run deposit=${dryRun.storageDeposit} fee=${fee.amount} required=$required")

                RegistrationCost(formExtrinsic = formExtrinsic, required = required)
            }
        }
    }

    private suspend fun register(
        chain: Chain,
        call: InstallationRegistrationCall,
        cost: RegistrationCost,
        target: InstallationRegistrationTarget,
    ): Result<DurableTxId> {
        coinageLogI("Installation registration: calling contract, ${target.logDescription()}")

        return extrinsicService.buildExtrinsic(chain, call.origin(), ExtrinsicService.SubmissionOptions(), cost.formExtrinsic)
            .flatMap { extrinsic -> durableTransactionService.submit(COINAGE_INSTALLATION_DOMAIN, extrinsic, target.registrationGroup()) {} }
            .onSuccess { id -> coinageLogI("Installation registration: submitted as durable tx ${id.value}") }
    }

    private fun InstallationRegistrationCall.origin(): TransactionOrigin = SignedTransactionOrigin(
        TransactionSignerSource.FromKeyPair(account.keypair, MultiChainEncryption.Substrate(EncryptionType.SR25519))
    )

    private fun InstallationRegistrationCall.formExtrinsic(weightLimit: WeightV2, storageDepositLimit: BigInteger): FormExtrinsic = {
        revive.call(dest = contract, weightLimit = weightLimit, storageDepositLimit = storageDepositLimit, data = input)
    }

    // The declared limits are what the fee is charged against, so they carry a margin rather than the pallet maximum.
    private fun WeightV2.withMargin() = WeightV2(refTime = refTime.withMargin(), proofSize = proofSize.withMargin())

    private fun BigInteger.withMargin(): BigInteger = (toBigDecimal() * (BigDecimal.ONE + LIMIT_MARGIN.fraction)).toBigInteger()

    private class RegistrationCost(
        val formExtrinsic: FormExtrinsic,
        val required: Balance,
    )

    private companion object {
        val LIMIT_MARGIN: Fraction = 20.percents
    }
}
