# Bounded Minitest parse-degraded fixture: a %-literal left open, so the file
# abstains whole-file with unterminated_percent_literal.
require "minitest/autorun"

class PercentTest < Minitest::Test
  def test_builds_word_list
    words = %w[alpha beta
  end
end
