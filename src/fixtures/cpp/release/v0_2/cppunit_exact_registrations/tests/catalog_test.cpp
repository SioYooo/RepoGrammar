#include <cppunit/extensions/HelperMacros.h>

// Three exact CppUnit suite registrations corroborated by the cppunit include,
// so each derives one bounded support fact and the three form a single family.
// The macro takes exactly one type name; the suite's own test methods are
// declared by CPPUNIT_TEST inside the class body and are deliberately not
// anchored, because enumerating them is registry construction at runtime.

class CatalogTest : public CppUnit::TestFixture
{
};

class OrderTest : public CppUnit::TestFixture
{
};

class ShipmentTest : public CppUnit::TestFixture
{
};

CPPUNIT_TEST_SUITE_REGISTRATION(CatalogTest);
CPPUNIT_TEST_SUITE_REGISTRATION(OrderTest);
CPPUNIT_TEST_SUITE_REGISTRATION(ShipmentTest);
