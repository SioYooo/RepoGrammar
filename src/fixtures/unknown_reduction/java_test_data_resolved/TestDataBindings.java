import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.DataProvider;
import org.testng.annotations.Test;

class ResolvedJunitDataTest {
    @ParameterizedTest
    @MethodSource("values")
    void reads(int value) {}

    static int[] values() {
        return new int[] { 1 };
    }
}

class ResolvedTestNgDataTest {
    @DataProvider(name = "rows")
    Object[][] rows() {
        return new Object[][] { { 1 } };
    }

    @Test(dataProvider = "rows")
    void reads(int value) {}
}
