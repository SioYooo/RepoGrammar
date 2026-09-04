# Bounded Minitest fixture: regexes in admitted positions, division after a
# closing bracket, character literals, and a squiggly heredoc terminator that
# is indented.
require "minitest/autorun"

class PricingTest < Minitest::Test
  def setup
    @prices = [10, 20, 30]
  end

  def test_applies_discount
    total = (@prices.sum) / 2
    assert_equal 30, total
    assert_match(/\d+/, total.to_s)
  end

  def test_formats_currency
    template = <<~PRICE
      amount: #{format("$%.2f", @prices.first)}
    PRICE
    assert_includes template, "amount:"
  end

  def test_accepts_single_character
    assert_equal "a", ?a
  end
end
