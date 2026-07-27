package de.shareui.harusdk

import android.content.Context
import de.shareui.haru.api.SdkStates
import de.shareui.harusdk.utils.Log

fun main(context: Context, self: SdkStates.Self) {
    Log.info("SDK de.shareui.harusdk loaded")
    Log.debug("self.state() = ${self.state()}")
    Log.debug("self.isInstalled() = ${self.isInstalled()}")
    Log.debug("self.isEnabled() = ${self.isEnabled()}")
    Log.debug("self.isRunning() = ${self.isRunning()}") // false
}
