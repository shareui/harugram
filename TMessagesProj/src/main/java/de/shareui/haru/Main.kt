package de.shareui.haru

import android.app.Application
import de.shareui.haru.api.HaruLog
import de.shareui.haru.sdk.SdkManager

class Main : Application() {
    override fun onCreate() {
        super.onCreate()
        HaruLog.log("Main Application started", debug = true)
        init() // sdks init
    }

    companion object {
        private var initialized = false

        // called from ApplicationLoader.onCreate, loads every enabled sdk
        @JvmStatic
        fun init() {
            if (initialized) {
                return
            }
            initialized = true
            try {
                SdkManager.initAll()
            } catch (e: Throwable) {
                HaruLog.log("SDK init failed: $e", HaruLog.Color.RED)
            }
        }
    }
}
