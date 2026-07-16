import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;
import org.testng.annotations.Test;

class UnresolvedJunitDataTest {
    @ParameterizedTest
    @MethodSource("example.ExternalRows#values")
    void reads(int value) {}
}

class UnresolvedTestNgDataTest {
    @Test(dataProvider = "rows", dataProviderClass = ExternalRows.class)
    void reads(int value) {}
}
