package com.example.loose;

// No JUnit or JPA imports: these annotations are lookalikes and must stay typed
// UNKNOWN, forming no family.
class LooseServiceTest {
    @Test
    void one() {}

    @Test
    void two() {}

    @Test
    void three() {}
}

@Entity
class LooseEntity {
    Long id;
}
