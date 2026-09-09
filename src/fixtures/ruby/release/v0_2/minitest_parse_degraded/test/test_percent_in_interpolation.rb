# Bounded Minitest parse-degraded fixture: a %-literal in code position inside
# a `#{}` interpolation. The declared subset admits a %-literal in ordinary
# code position, but refuses one inside an interpolation, because deciding
# where its delimiter closes would require the disambiguation the subset does
# not perform. The file abstains whole-file with unadmitted_construct rather
# than admitting a boundary it cannot prove.
require "minitest/autorun"

class InterpolatedPercentTest < Minitest::Test
  def test_counts_words
    summary = "n: #{ %w[alpha beta].length }"
    assert_includes summary, "n:"
  end
end
