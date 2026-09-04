package degraded

import "testing"

// The braces never balance, so the declaration extents in this file are
// unproven and the whole file abstains.

func TestUnbalanced(t *testing.T) {
	if true {
}
