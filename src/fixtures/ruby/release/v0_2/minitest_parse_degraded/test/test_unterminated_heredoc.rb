# Bounded Minitest parse-degraded fixture: a heredoc whose body never reaches
# its terminator, so every later token boundary is unproven and the file
# abstains whole-file with unterminated_heredoc.
require "minitest/autorun"

class OpenTest < Minitest::Test
  def test_reads_document
    text = <<~NEVER_CLOSED
      the terminator line below never appears
  end
end
