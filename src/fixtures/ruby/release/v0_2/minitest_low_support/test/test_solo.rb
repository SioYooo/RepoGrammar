# Bounded Minitest low-support fixture: two anchors, one below the family
# minimum of three.
require "minitest/autorun"

class SoloTest < Minitest::Test
  def test_runs_alone
    assert_equal 1, 1
  end

  def test_runs_again
    assert_equal 1, 1
  end
end
