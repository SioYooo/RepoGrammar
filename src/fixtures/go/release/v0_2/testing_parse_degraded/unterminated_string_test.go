package degraded

import "testing"

// The interpreted string below never closes, so the token stream after it is
// literal text and every later boundary is unproven.

func TestUnterminated(t *testing.T) {
	note := "never closed
}
