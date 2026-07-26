package de.shareui.haru.api

import de.shareui.haru.Sdk.HaruSdk
import de.shareui.haru.Sdk.SdkManager
import org.telegram.messenger.AndroidUtilities
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Reads and drives the state of installed SDKs.
 *
 * Like [HaruLog] this is a plain call surface — an SDK's dex is loaded with the
 * app class loader as parent, so it reaches this object directly:
 *
 * ```
 * if (SdkStates.isRunning("de.shareui.other")) { ... }
 *
 * SdkStates.addListener { id, event ->
 *     if (id == "de.shareui.harusdk" && event == SdkStates.Event.STOPPED) tearDown()
 * }
 * ```
 *
 * An SDK is described by two independent facts: whether the user switched it on
 * ([isEnabled], persisted across restarts) and whether its dex is loaded in this
 * process right now ([isRunning]). [stateOf] folds both into one value.
 */
object SdkStates {

    /** The four states an SDK id can be in, from the app's point of view. */
    enum class State {
        /** Nothing is unpacked under this id. */
        NOT_INSTALLED,

        /** Installed, but the user switched it off. */
        DISABLED,

        /**
         * Enabled, yet its dex is not loaded in this process — it either failed
         * to start, or it was switched off and back on without a restart.
         */
        STOPPED,

        /** Loaded, and its entry point ran. */
        RUNNING;

        val isInstalled: Boolean get() = this != NOT_INSTALLED

        val isEnabled: Boolean get() = this == STOPPED || this == RUNNING
    }

    /** What happened to an SDK. Delivered to every [Listener] on the UI thread. */
    enum class Event {
        INSTALLED,
        UPDATED,
        ENABLED,
        DISABLED,

        /** The dex was loaded and the entry point returned without throwing. */
        STARTED,

        /**
         * The teardown hook ran and the class loader was dropped. Android cannot
         * unmap a dex, so the code stays in memory until the app restarts.
         */
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

    /** The user's switch, as persisted — true even while the SDK is not loaded. */
    @JvmStatic
    fun isEnabled(id: String): Boolean = isInstalled(id) && SdkManager.isEnabled(id)

    /** True only while this SDK's dex is loaded in the current process. */
    @JvmStatic
    fun isRunning(id: String): Boolean = SdkManager.isRunning(id)

    /** Everything unpacked under `{filesDir}/sdk/`, with its metadata. */
    @JvmStatic
    fun installed(): List<HaruSdk> = SdkManager.list()

    /** Ids only, for callers that just want to test for presence. */
    @JvmStatic
    fun installedIds(): List<String> = SdkManager.list().map { it.id }

    @JvmStatic
    fun find(id: String): HaruSdk? = SdkManager.find(id)

    // endregion

    // region lifecycle

    /**
     * Flips the user's switch and applies it right away: enabling loads the SDK
     * now, disabling runs its teardown hook.
     *
     * @return the failure, or null when it worked (or was a disable)
     */
    @JvmStatic
    fun setEnabled(id: String, enabled: Boolean): Throwable? {
        val sdk = SdkManager.find(id)
            ?: return IllegalArgumentException("no sdk installed as $id")
        return SdkManager.setEnabled(sdk, enabled)
    }

    /**
     * Loads [id]'s dex and calls its entry point, without touching the user's
     * switch. Returns the failure, or null on success — including when it was
     * already running.
     */
    @JvmStatic
    fun start(id: String): Throwable? {
        val sdk = SdkManager.find(id) ?: return IllegalArgumentException("no sdk installed as $id")
        return SdkManager.start(sdk)
    }

    /**
     * Runs [id]'s teardown hook and drops its class loader, without touching the
     * user's switch. Returns false when it was not running.
     */
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

    /**
     * Posts [event] to every listener. Called by `SdkManager` as the state
     * actually changes — SDKs listen, they do not dispatch.
     */
    @JvmStatic
    fun dispatch(id: String, event: Event) {
        if (listeners.isEmpty()) {
            return
        }
        AndroidUtilities.runOnUIThread {
            for (listener in listeners) {
                // One bad listener must not stop the others from being told.
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
