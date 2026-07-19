package de.shareui.harusdk

fun ktStdlibTest() {
    val numbers = listOf(4, 2, 7, 1, 9, 3)
    val sorted = numbers.sorted()
    val sum = numbers.sum()
    val evens = numbers.filter { it % 2 == 0 }
    println("Sorted: $sorted, sum: $sum, evens: $evens")
}
