package de.shareui.harusdk.utils

import de.shareui.haru.api.HaruLog

// simple HaruLog wrapper
object Log {
    fun log(
        text: String,
        color: HaruLog.Color = HaruLog.Color.DEFAULT,
        debug: Boolean = false
    ) = HaruLog.log(text, color, debug)
    
    fun debug(text: String) = log(text, HaruLog.Color.DEFAULT, debug = true)
    fun info(text: String) = log(text, HaruLog.Color.DEFAULT)
    fun success(text: String) = log(text, HaruLog.Color.GREEN)
    fun warn(text: String) = log(text, HaruLog.Color.YELLOW)
    fun error(text: String) = log(text, HaruLog.Color.RED)
}
