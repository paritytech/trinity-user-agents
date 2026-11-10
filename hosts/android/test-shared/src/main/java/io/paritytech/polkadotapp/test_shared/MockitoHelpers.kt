package io.paritytech.polkadotapp.test_shared

import io.paritytech.polkadotapp.common.utils.InformationSize
import org.mockito.ArgumentCaptor
import org.mockito.ArgumentMatcher
import org.mockito.Mockito
import org.mockito.invocation.Invocation
import org.mockito.invocation.InvocationOnMock
import org.mockito.stubbing.OngoingStubbing
import kotlin.coroutines.Continuation
import kotlin.coroutines.intrinsics.startCoroutineUninterceptedOrReturn

/**
 * Returns Mockito.eq() as nullable type to avoid java.lang.IllegalStateException when
 * null is returned.
 *
 * Generic T is nullable because implicitly bounded by Any?.
 */
fun <T> eq(obj: T): T = Mockito.eq<T>(obj)

/**
 * Returns Mockito.any() as nullable type to avoid java.lang.IllegalStateException when
 * null is returned.
 */
fun <T> any(): T = Mockito.any<T>()

/**
 * Returns Mockito.isA() as nullable type to avoid java.lang.IllegalStateException when
 * null is returned.
 */
fun <T> isA(classRef: Class<T>): T = Mockito.isA<T>(classRef)

fun <T> capture(captor: ArgumentCaptor<T>): T = captor.capture()

fun <T> argThat(argumentMatcher: ArgumentMatcher<T>): T = Mockito.argThat<T>(argumentMatcher)

fun <T> whenever(methodCall: T): OngoingStubbing<T> =
    Mockito.`when`(methodCall)

fun <T> OngoingStubbing<T>.thenThrowUnsafe(exception: Exception) = thenAnswer {
    throw exception
}

@Suppress("UnusedReturnValue")
fun anyUInt(): UInt {
    Mockito.argThat<Any?> { true }
    return 0u
}

fun eqUInt(value: UInt): UInt {
    Mockito.argThat<Any?> { actual -> actual?.toString() == value.toString() }
    return value
}

/** Argument matcher for [Long] — wraps Mockito's primitive-friendly matcher. */
fun anyLong(): Long = Mockito.anyLong()

/** Argument matcher for [Int] — wraps Mockito's primitive-friendly matcher. */
fun anyInt(): Int = Mockito.anyInt()

fun anyInformationSize(): InformationSize {
    Mockito.anyLong()
    return InformationSize.ZERO
}

fun <T> OngoingStubbing<T>.thenSuspend(answer: suspend (InvocationOnMock) -> T): OngoingStubbing<T> = thenAnswer { invocation ->
    @Suppress("UNCHECKED_CAST")
    val continuation = (invocation as Invocation).rawArguments.last() as Continuation<Any?>
    val call: suspend () -> T = { answer(invocation) }
    call.startCoroutineUninterceptedOrReturn(continuation)
}
