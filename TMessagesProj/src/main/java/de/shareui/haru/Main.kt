package de.shareui.haru

import android.app.Application
import android.util.Log
import de.shareui.haru.Sdk.SdkManager

// no tasks for now
class Main : Application() {
    override fun onCreate() {
        super.onCreate()
        Log.d("Haru", "Main Application started")
        init()
    }

    companion object {
        private var initialized = false

        /**
         * Haru's app-start hook — loads every enabled SDK from
         * `{filesDir}/sdk/`. Called from `ApplicationLoader.onCreate`, since
         * that is the Application the manifest actually registers.
         */
        @JvmStatic
        fun init() {
            if (initialized) {
                return
            }
            initialized = true
            try {
                SdkManager.initAll()
            } catch (e: Throwable) {
                // A broken SDK must never take the app down with it.
                Log.e("Haru", "SDK init failed", e)
            }
        }
    }
}
