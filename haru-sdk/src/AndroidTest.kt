package de.shareui.harusdk

import android.util.Base64

fun androidTest() {
    val encoded = Base64.encodeToString("haru".toByteArray(), Base64.NO_WRAP)
    println("Base64 of \"haru\": $encoded")
}
