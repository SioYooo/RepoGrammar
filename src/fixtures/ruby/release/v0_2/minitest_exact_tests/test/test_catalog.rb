# Bounded Minitest fixture: the ADR-0049 admitted shape over hostile literals.
# Hostile constructs that must still parse: a squiggly heredoc whose body holds
# a class and def line, a nested string inside an interpolation, a %w list with
# nested delimiters in code position, quoted symbols, and a regex in admitted
# position. A %-literal *inside* an interpolation is outside the declared
# subset by design, so it lives in the parse-degraded fixture instead.
require "minitest/autorun"

class CatalogTest < Minitest::Test
  def setup
    @catalog = %w[apple banana cherry [nested]]
    @labels = { ready: :"yes", paused: :'not now' }
  end

  def test_loads_catalog
    plan = <<~PLAN
      class Fake < Minitest::Test
        def test_hidden_inside_heredoc
    PLAN
    summary = "items: #{@catalog.length} of #{ ["a", "b"].length } total"
    assert_equal 3, @catalog.length
    assert_includes summary, "items: 3"
    refute_includes plan, "cherry"
  end

  def test_filters_catalog
    selected = @catalog.select { |item| item.start_with?("b") }
    assert_match(/b+a+n+a+n/, selected.first)
    assert_equal (2 + 2) / 2, 2
  end

  def test_labels_catalog
    assert_equal :yes, @labels[:ready]
    assert_equal "not now", @labels[:paused].to_s
  end
end
