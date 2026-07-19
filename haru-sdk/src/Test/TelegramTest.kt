package de.shareui.harusdk

import org.telegram.messenger.UserConfig

fun telegramTest() {
    val activatedAccounts = UserConfig.getActivatedAccountsCount()
    val selected = UserConfig.selectedAccount
    println("Activated accounts: $activatedAccounts, selected: $selected")
}
