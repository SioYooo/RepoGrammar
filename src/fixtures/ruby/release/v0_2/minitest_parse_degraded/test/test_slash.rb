# Bounded Minitest parse-degraded fixture: division after a mid-statement
# identifier. MRI itself decides this from the runtime local/command
# distinction, and either reading moves structural boundaries, so the file
# abstains whole-file with ruby_slash_disambiguation.
require "minitest/autorun"

class RatioTest < Minitest::Test
  def test_computes_ratio
    expected = 4
    ratio = expected / 2
    assert_equal 2, ratio
  end
end
