# Bounded Minitest fixture: the superclass chain resolves within the file
# (ADR-0049 D3) and the require context records test-unit rather than
# minitest/autorun.
require "test-unit"

class OrdersBaseTest < Minitest::Test
  def build_order(quantity)
    { quantity: quantity }
  end
end

class OrdersTest < OrdersBaseTest
  def test_places_order
    order = build_order(2)
    assert_equal 2, order[:quantity]
  end

  def test_rejects_empty_order
    order = build_order(0)
    assert_equal 0, order[:quantity]
  end
end
