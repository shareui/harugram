package de.shareui.harusdk.utils

import de.shareui.haru.api.HaruLog

/**
 * Writes into Haru's log fragment (Settings → Debug menu → Open logs).
 *
 * [HaruLog] lives in the app and the SDK's dex is loaded with the app class
 * loader as its parent, so this is a plain call — nothing is hooked, nothing is
 * looked up by reflection.
 *
 * A line carries a color (default/green/yellow/red) and a debug flag; debug
 * lines are only listed while "Verbose logging" is on.
 */
object Log {

    /**
     * @param text what to log
     * @param color default, green, yellow or red
     * @param debug true for a line that only matters while debugging
     */
    fun log(
        text: String,
        color: HaruLog.Color = HaruLog.Color.DEFAULT,
        debug: Boolean = false
    ) = HaruLog.log(text, color, debug)

    /** Debug line, hidden unless verbose logging is on. */
    fun debug(text: String) = log(text, HaruLog.Color.DEFAULT, debug = true)

    fun info(text: String) = log(text, HaruLog.Color.DEFAULT)

    fun success(text: String) = log(text, HaruLog.Color.GREEN)

    fun warn(text: String) = log(text, HaruLog.Color.YELLOW)

    fun error(text: String) = log(text, HaruLog.Color.RED)
}
