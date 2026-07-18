package de.shareui.haru

import android.app.Application
import android.util.Log

// no tasks for now
class Main : Application() {
    override fun onCreate() {
        super.onCreate()
        Log.d("Haru", "Main Application started")
    }
}