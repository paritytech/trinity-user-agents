package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Turns the studio with the phone, so the light behaves as though it were fixed in the room.
 *
 * The environment the coins are lit against is defined around the coin's own space, so with the studio held
 * still the highlight sits in the same place however the phone is held. Feeding the device's own tilt back
 * in turns the studio the other way, and a coin catches the light as you tilt it, the way a real one does.
 *
 * A sideways movement rolls the studio about the axis you are looking down; leaning the phone back pitches
 * it. That first part is a deliberate choice, not physics, and it is the fix for a light that responded to
 * one direction of tilt and not the other.
 *
 * Turning the studio the way the phone really turned puts a sideways movement about the screen's long axis,
 * which slides the reflection horizontally. This studio is a room: bright above, dark below, and nearly even
 * from side to side. Measured across the whole face of a coin, forty degrees of that turn changes what it
 * reflects by 0.31 one way and by 0.10 the other — a dead side, and correcting the sense of the turn only
 * moved the dead side to the other hand. Rolling the room about the view axis changes it by 0.18 either way,
 * symmetric by construction: a roll about the axis the coin faces cannot care which way it went.
 *
 * It is also what the eye expects. The head does not roll with the phone, so relative to the screen the room
 * is what rolls, and a coin's highlight sweeping around its face is the thing that reads as catching light.
 *
 * `TYPE_GRAVITY` needs no permission and no prompt. It is a fused reading and not present on every device,
 * so the plain accelerometer stands in; at rest the two agree, which is when the studio is allowed to move.
 */
class CoinageTilt(context: Context) : SensorEventListener {
    /**
     * Where the studio is turned to: an axis scaled by the angle turned about it, in radians.
     *
     * One vector rather than a pair of angles, because a rotation about a slanted axis is not the sum of two
     * rotations about the axes either side of it, and the phone rotates about whatever axis the wrist chose.
     */
    data class Turn(val x: Double = 0.0, val y: Double = 0.0, val z: Double = 0.0)

    /**
     * One reading: which way is down, in the phone's own frame.
     *
     * Gravity is all the studio needs and all it can have. It fixes the phone's orientation up to a turn
     * about the vertical, which is exactly the turn a room's lighting does not care about, and unlike
     * integrated attitude it never drifts.
     */
    class Pose private constructor(val downX: Double, val downY: Double, val downZ: Double) {
        /**
         * How far the right edge has dipped, and how far the screen has leaned back from vertical.
         *
         * Each against everything left over rather than against one other component. Anything of the form
         * `atan2(x, y)` between two components that both shrink is a bearing, and a bearing is
         * ill-conditioned wherever the phone is leaned well back — the way it is held to look at something.
         * A five degree roll once read as five degrees upright, twenty-seven leaned back and a hundred and
         * seventy past flat.
         */
        val sideways: Double get() = atan2(downX, hypot(downY, downZ))
        val lean: Double get() = atan2(-downZ, hypot(downX, downY))

        /**
         * How far the phone turned between two readings, whatever axis it turned about. Against the cross
         * product rather than from the dot alone: `acos` loses most of its precision for the small angles
         * between consecutive readings, which is where this is asked most.
         */
        fun angleTo(other: Pose): Double {
            val crossX = downY * other.downZ - downZ * other.downY
            val crossY = downZ * other.downX - downX * other.downZ
            val crossZ = downX * other.downY - downY * other.downX
            val dot = downX * other.downX + downY * other.downY + downZ * other.downZ

            return atan2(sqrt(crossX * crossX + crossY * crossY + crossZ * crossZ), dot)
        }

        /** Where the studio sits with the phone here, against this reading as square. */
        fun turnTo(other: Pose): Turn =
            composed(pitch = scaled(other.lean - lean), roll = scaled(other.sideways - sideways))

        /**
         * Eases toward another reading, staying a direction rather than shrinking toward the middle: neutral
         * is which way is down, and a blend of two directions has no length of its own to keep.
         */
        fun blended(other: Pose, blend: Double) = of(
            downX + (other.downX - downX) * blend,
            downY + (other.downY - downY) * blend,
            downZ + (other.downZ - downZ) * blend
        )

        companion object {
            fun of(x: Double, y: Double, z: Double): Pose {
                val length = maxOf(sqrt(x * x + y * y + z * z), 1e-9)

                return Pose(x / length, y / length, z / length)
            }

            /**
             * Android reports gravity as proper acceleration — it points *away* from the earth, the opposite
             * of iOS's `CMDeviceMotion.gravity`. The axes are otherwise the same, so negating is the whole
             * of the conversion.
             */
            fun ofSensor(values: FloatArray) =
                of(-values[0].toDouble(), -values[1].toDouble(), -values[2].toDouble())
        }
    }

    private val sensors = context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
    private val sensor = sensors?.let {
        it.getDefaultSensor(Sensor.TYPE_GRAVITY) ?: it.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
    }

    private var neutral: Pose? = null
    private var previous: Pose? = null
    private var stillFor = 0.0
    private var lastReading = 0L
    private var target = Turn()

    @Volatile
    var turn = Turn()
        private set

    /**
     * Called when the phone has moved enough that the studio needs to catch up. The view pauses itself
     * whenever nothing is moving, so without a push from outside it would never look at the attitude again
     * and the light would stay where the field last settled.
     */
    var onMove: (() -> Unit)? = null

    fun start() {
        val sensor = sensor ?: return

        sensors?.registerListener(this, sensor, INTERVAL_MICROSECONDS)
    }

    /**
     * Lets go of the sensor. A pause rather than a teardown: [onMove] is the caller's and survives, so the
     * same instance can be started again when the card comes back on screen.
     *
     * Neutral does not survive. A phone that was put somewhere else while nobody was looking should come
     * back square, and the first reading after starting is taken as neutral outright.
     */
    fun stop() {
        sensors?.unregisterListener(this)
        neutral = null
        previous = null
        stillFor = 0.0
        lastReading = 0L
    }

    /**
     * Eases toward the latest turn. Returns whether the studio is still catching up, so a field at rest
     * under a still hand goes back to sleep.
     */
    fun advance(seconds: Float): Boolean {
        if (isSettled) return false

        val blend = minOf(seconds.toDouble() * SMOOTHING, 1.0)

        turn = Turn(
            x = turn.x + (target.x - turn.x) * blend,
            y = turn.y + (target.y - turn.y) * blend,
            z = turn.z + (target.z - turn.z) * blend
        )

        return true
    }

    override fun onSensorChanged(event: SensorEvent) {
        // The sampling period is only a hint, so the real gap decides how much of it counts as stillness.
        val elapsed = if (lastReading == 0L) {
            INTERVAL_SECONDS
        } else {
            ((event.timestamp - lastReading) / NANOSECONDS).coerceIn(MIN_INTERVAL, MAX_INTERVAL)
        }
        lastReading = event.timestamp

        absorb(Pose.ofSensor(event.values), elapsed)
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit

    /** Folds one reading into the neutral and works out where the studio should sit against it. */
    fun absorb(pose: Pose, seconds: Double) {
        val previous = previous
        var settled = neutral ?: pose

        if (previous != null && neutral != null) {
            stillFor = if (previous.angleTo(pose) / maxOf(seconds, 1e-4) < STILL_SPEED) {
                stillFor + seconds
            } else {
                0.0
            }

            // Put down somewhere new, so wherever it is becomes square. Still being moved, or only paused
            // mid-gesture, so neutral waits and the movement reads in full.
            if (stillFor >= DWELL) {
                settled = settled.blended(pose, minOf(seconds / SETTLE, 1.0))
            }
        } else {
            // The first reading is neutral outright, so the light starts square rather than drifting in from
            // wherever the phone happened to be picked up.
            settled = pose
        }

        neutral = settled
        this.previous = pose
        target = settled.turnTo(pose)

        if (isWorthWaking) onMove?.invoke()
    }

    /** Where the studio is against where it is heading, on whichever axis is furthest off. */
    private val offset: Double
        get() = maxOf(abs(target.x - turn.x), abs(target.y - turn.y), abs(target.z - turn.z))

    /** Arrived: close enough that another frame would move nothing. */
    private val isSettled: Boolean get() = offset <= REST_BAND

    /**
     * Far enough out to be worth a frame.
     *
     * Deliberately much coarser than [isSettled]. Waking on the resting threshold meant waking on anything
     * at all: it works out to four hundredths of a degree of tilt, which is under the sensor's own noise
     * floor, so a phone merely held in a hand woke the renderer sixty times a second and redrew every coin
     * for as long as the card was open. This is only the lighting — a degree of it is not visible — and
     * once awake the studio still eases the whole way in, so a real movement is as smooth as it ever was.
     */
    private val isWorthWaking: Boolean get() = offset > WAKE_BAND

    companion object {
        /**
         * How far the studio turns at the ends of the travel. A whole rotation would swing the highlight
         * right around the coin, which reads as a spinning lamp rather than a coin in a hand.
         */
        const val TRAVEL = 0.9

        /**
         * The tilt that reaches the ends of the travel. Deliberately short of a right angle: nobody turns a
         * phone ninety degrees to look at it, and the whole range should be within a wrist's movement.
         */
        const val RANGE = PI / 4

        /**
         * How quickly a phone that is being held still becomes the new neutral.
         *
         * Zero offset is where the studio sits as it was calibrated: square to the screen, which is where
         * the coins have most contrast. Any fixed idea of how a phone is held is wrong for somebody, so
         * neutral has to follow.
         *
         * It follows only while the phone is still. An average over the last few seconds cannot tell a
         * deliberate tilt from a new resting position, so it took both: a two second tilt dragged neutral
         * thirteen degrees, and putting the phone back where it started left a dozen degrees of light that
         * should not have been there. Worse, that bias ate one side of the travel.
         */
        const val SETTLE = 1.0

        /** Above this the phone is being moved rather than held, and neutral waits. */
        const val STILL_SPEED = 0.25

        /**
         * How long the phone has to be still before it counts as put down somewhere new. A hand pauses at
         * the end of a gesture before bringing the phone back, and without this that pause starts moving
         * neutral.
         */
        const val DWELL = 0.4

        /**
         * Chases the device rather than tracking it exactly: raw attitude is noisy enough to make a specular
         * highlight shimmer while the phone is held still.
         */
        const val SMOOTHING = 8.0

        /** Arrived. Fine, because it only decides when to stop easing, not when to start. */
        private const val REST_BAND = 0.0008

        /**
         * A degree of the phone's own tilt, converted into the studio's turn.
         *
         * An order of magnitude above hand tremor and sensor noise, and an order below a deliberate
         * movement, which is the whole width of the gap this has to sit in.
         */
        private val WAKE_BAND = TRAVEL / RANGE * (PI / 180)

        private const val INTERVAL_MICROSECONDS = 16_667
        private const val INTERVAL_SECONDS = 1.0 / 60
        private const val MIN_INTERVAL = 1.0 / 240
        private const val MAX_INTERVAL = 1.0 / 20
        private const val NANOSECONDS = 1_000_000_000.0

        /**
         * Scales a turn of `angle` into the travel, so the ends of the wrist's range reach the ends of the
         * studio's and no tilt can push it further.
         */
        fun scaled(angle: Double): Double = angle.coerceIn(-RANGE, RANGE) / RANGE * TRAVEL

        /**
         * The two turns as one, in the order they are felt: the room rolls about the view axis, and the
         * whole of it is then pitched by however far the phone is leaned back.
         *
         * Composed as rotations rather than added as a vector. Turns about different axes do not add, and
         * these two are large enough at the ends of the travel for the difference to show.
         */
        fun composed(pitch: Double, roll: Double): Turn {
            // Quaternion about x, then about z, as a product; then read back as axis times angle.
            val halfPitch = pitch / 2
            val halfRoll = roll / 2
            val cosPitch = cos(halfPitch)
            val sinPitch = sin(halfPitch)
            val cosRoll = cos(halfRoll)
            val sinRoll = sin(halfRoll)

            val w = cosPitch * cosRoll
            val x = sinPitch * cosRoll
            // The cross term of the two axes, which is exactly what makes a product of rotations differ
            // from a sum of them.
            val y = -sinPitch * sinRoll
            val z = cosPitch * sinRoll

            val sine = sqrt(x * x + y * y + z * z)

            if (sine < 1e-9) return Turn()

            val angle = 2 * atan2(sine, w)

            return Turn(x / sine * angle, y / sine * angle, z / sine * angle)
        }
    }
}
