# Bounded Minitest lookalike fixture: every class here must fail the anchor
# for a different declared reason, and the fakes inside strings, comments, and
# heredocs must never become tokens.
#
# A class with no superclass and a test-shaped method: the framework identity
# is unproven and the file records test_methods_without_minitest_base.
class HelperService
  def test_loads
    :no_base
  end
end

# A class whose superclass does not resolve within the file.
class WidgetTest < BaseTest
  def test_renders
    :unknown_base
  end
end

# The real base, but every method shape here is outside the admitted anchor.
class ShadowTest < Minitest::Test
  # A class method, not an instance method.
  def self.test_class_method
    :class_method
  end

  # Parameters: the runner could not invoke it with zero arguments.
  def test_with_parameters(count)
    :parameterized
  end

  # No test_ prefix.
  def helper_without_prefix
    :wrong_prefix
  end

  # Not a direct statement of the class body.
  private def test_visibility_prefixed
    :not_direct
  end

  # The one admitted method in this file.
  def test_real
    assert true
  end
end

# Fakes that must stay inert: class headers inside a comment, a string, and a
# squiggly heredoc.
# class CommentedTest < Minitest::Test
#   def test_from_comment; end
# end
FAKE = "class StringTest < Minitest::Test"
fake_body = <<~FAKE
  class HeredocTest < Minitest::Test
    def test_from_heredoc
  FAKE
