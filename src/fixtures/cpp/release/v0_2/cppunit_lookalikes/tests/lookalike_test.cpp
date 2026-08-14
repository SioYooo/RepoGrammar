// No cppunit include, so the registration macro name proves nothing. A project
// may define its own macro with this spelling; without include corroboration
// the identity is UNKNOWN and no anchor may form.

class CatalogTest
{
};

class OrderTest
{
};

class ShipmentTest
{
};

CPPUNIT_TEST_SUITE_REGISTRATION(CatalogTest);
CPPUNIT_TEST_SUITE_REGISTRATION(OrderTest);
CPPUNIT_TEST_SUITE_REGISTRATION(ShipmentTest);
