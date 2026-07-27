package de.shareui.haru.api

import de.shareui.haru.sdk.HaruSdk
import de.shareui.haru.sdk.SdkManager
import org.telegram.messenger.AndroidUtilities
import java.util.concurrent.CopyOnWriteArrayList

// reads and drives the state of installed sdks
// isEnabled is the persisted user switch, isRunning is whether the dex is loaded right now
object SdkStates {

    enum class State {
        NOT_INSTALLED,
        DISABLED,

        // enabled but not loaded: either failed to start, or toggled without a restart
        STOPPED,
        RUNNING;

        val isInstalled: Boolean get() = this != NOT_INSTALLED

        val isEnabled: Boolean get() = this == STOPPED || this == RUNNING
    }

    // delivered to every listener on the ui thread
    enum class Event {
        INSTALLED,
        UPDATED,
        ENABLED,
        DISABLED,
        STARTED,

        // android cannot unmap a dex, so the code stays in memory until restart
        STOPPED,
        UNINSTALLED
    }

    fun interface Listener {
        fun onSdkEvent(id: String, event: Event)
    }

    private val listeners = CopyOnWriteArrayList<Listener>()

    // region state

    @JvmStatic
    fun stateOf(id: String): State = when {
        !isInstalled(id) -> State.NOT_INSTALLED
        !SdkManager.isEnabled(id) -> State.DISABLED
        SdkManager.isRunning(id) -> State.RUNNING
        else -> State.STOPPED
    }

    @JvmStatic
    fun isInstalled(id: String): Boolean = SdkManager.find(id) != null

    @JvmStatic
    fun isEnabled(id: String): Boolean = isInstalled(id) && SdkManager.isEnabled(id)

    @JvmStatic
    fun isRunning(id: String): Boolean = SdkManager.isRunning(id)

    @JvmStatic
    fun installed(): List<HaruSdk> = SdkManager.list()

    @JvmStatic
    fun installedIds(): List<String> = SdkManager.list().map { it.id }

    @JvmStatic
    fun find(id: String): HaruSdk? = SdkManager.find(id)

    // endregion

    // region lifecycle

    // flips the user's switch and applies it right away
    @JvmStatic
    fun setEnabled(id: String, enabled: Boolean): Throwable? {
        val sdk = SdkManager.find(id)
            ?: return IllegalArgumentException("no sdk installed as $id")
        return SdkManager.setEnabled(sdk, enabled)
    }

    @JvmStatic
    fun start(id: String): Throwable? {
        val sdk = SdkManager.find(id) ?: return IllegalArgumentException("no sdk installed as $id")
        return SdkManager.start(sdk)
    }

    @JvmStatic
    fun stop(id: String): Boolean {
        val sdk = SdkManager.find(id) ?: return false
        return SdkManager.stop(sdk)
    }

    // endregion

    // region listeners

    fun addListener(listener: Listener) {
        listeners.addIfAbsent(listener)
    }

    fun removeListener(listener: Listener) {
        listeners.remove(listener)
    }

    @JvmStatic
    fun dispatch(id: String, event: Event) {
        if (listeners.isEmpty()) {
            return
        }
        AndroidUtilities.runOnUIThread {
            for (listener in listeners) {
                // one bad listener must not stop the others from being told
                try {
                    listener.onSdkEvent(id, event)
                } catch (e: Throwable) {
                    HaruLog.log("sdk listener failed on $id/$event: $e", HaruLog.Color.RED, true)
                }
            }
        }
    }

    // endregion
}
