package de.shareui.haru

import de.shareui.haru.api.HaruLog
import de.shareui.haru.sdk.SdkManager

object Main {
    private var initialized = false
    // ApplicationLoader.onCreate
    @JvmStatic
    fun init() {
        if (initialized) {
            return
        }
        initialized = true
        HaruLog.App.log("Main Application started", debug = true)
        try {
            SdkManager.initAll()
        } catch (e: Throwable) {
            HaruLog.App.log("SDK init failed: $e", HaruLog.Color.RED)
        }
    }
}
