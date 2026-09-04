package degraded

import "testing"

// A generic signature is outside the declared subset: the type-parameter list
// is undecidable by the bounded parser, so the whole file abstains.

func TestGeneric[T any](t *testing.T) {
}
