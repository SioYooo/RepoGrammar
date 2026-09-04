package tabledriven

import "testing"

// A table-driven test: the t.Run subtests live in the body, which the bounded
// parser skips, so the declaration itself is the anchor and the prose inside
// the body never contributes one.

func TestSelectsRows(t *testing.T) {
	cases := []struct {
		name string
		in   int
	}{
		{"zero", 0},
		{"negative", -1},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if c.in < 0 {
				t.Errorf("negative input: %d", c.in)
			}
		})
	}
}

func TestTableProse(t *testing.T) {
	// func TestCommented(t *testing.T) {}
	golden := `func TestRaw(t *testing.T) {}`
	quoted := "func TestQuoted(t *testing.T) {}"
	_ = golden
	_ = quoted
}

func TestFiltersRows(t *testing.T) {
	t.Run("keeps", func(t *testing.T) {})
}
