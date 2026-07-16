package com.example;

// The @Test simple name has no exact import, so each method stays typed UNKNOWN
// (UnresolvedImport / java_test_annotation_binding) and forms no family.
class JunitUnresolvedTest {
    @Test
    void alpha() {}

    @Test
    void beta() {}

    @Test
    void gamma() {}
}
