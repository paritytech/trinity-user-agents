package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.TransitionResult
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import java.math.BigInteger

val PAYMENT = PaymentContext(
    key = ExternalPaymentKey(origin = "product.dot", id = "0x01"),
    amount = planks(10),
    destination = byteArrayOf(7).intoAccountId(),
)

fun voucherInRecycler(index: Int, exponent: Int) = RecyclerVoucher(
    ringVrfKeyIndex = testKey(index),
    ringVrfPublicKey = byteArrayOf(index.toByte()).toDataByteArray(),
    recyclerValue = ValueExponent(exponent),
    location = RecyclerVoucher.Location.InRecycler(RecyclerIndex(BigInteger.ONE), recyclerMembers = 32, enteredAt = null),
    recyclerFungibility = RecyclerFungibility.NONE,
    maxRecyclerFungibility = RecyclerFungibility.NONE,
)

suspend fun ExternalPaymentState.transition(): TransitionResult<ExternalPaymentState> = with(Unit) { performTransition() }

/** The state a transition moved to, or the failure it left for the worker to retry. */
fun TransitionResult<ExternalPaymentState>.outcome(): Result<ExternalPaymentState> =
    (this as TransitionResult.TransitionPerformed).outcome
