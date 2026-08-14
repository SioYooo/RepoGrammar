package lookalikes

// No testing import, so nothing in this file declares a Go test. The
// declaration text also appears as prose, which must never anchor.

// func TestCommented(t *testing.T) {}
/* func TestBlockCommented(t *testing.T) {} */

var golden = `func TestRaw(t *testing.T) {}`
var quoted = "func TestQuoted(t *testing.T) {}"

func TestLoadsCatalog(t *T) {
}

func TestFiltersCatalog(t *T) {
}

func TestOpensProduct(t *T) {
}

type T struct{}
