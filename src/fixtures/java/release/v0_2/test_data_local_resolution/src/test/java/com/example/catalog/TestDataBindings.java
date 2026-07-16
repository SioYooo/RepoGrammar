package com.example.catalog;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class JunitDataTest {
    @ParameterizedTest
    @MethodSource("firstRows")
    void readsFirst(int value) {}

    static int[] firstRows() {
        return new int[] { 1 };
    }

    @ParameterizedTest
    @MethodSource("secondRows")
    void readsSecond(int value) {}

    static int[] secondRows() {
        return new int[] { 2 };
    }

    @ParameterizedTest
    @MethodSource
    void readsThird(int value) {}

    static int[] readsThird() {
        return new int[] { 3 };
    }
}

class TestNgDataTest {
    @DataProvider(name = "first")
    Object[][] firstRows() {
        return new Object[][] { { 1 } };
    }

    @Test(dataProvider = "first")
    void readsFirst(int value) {}

    @DataProvider(name = "second")
    Object[][] secondRows() {
        return new Object[][] { { 2 } };
    }

    @Test(dataProvider = "second")
    void readsSecond(int value) {}

    @DataProvider
    Object[][] thirdRows() {
        return new Object[][] { { 3 } };
    }

    @Test(dataProvider = "thirdRows")
    void readsThird(int value) {}
}
