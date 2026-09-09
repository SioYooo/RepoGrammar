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

// A lowercase suffix does not name a test `go test` would run.
func Testx(t *T) {}

// Two parameters are not the one-parameter test shape.
func TestPair(t *T, extra int) {}

// A result is not the resultless test shape.
func TestResult(t *T) error { return nil }

// The callee is an ordinary helper despite the Test prefix.
func Testify(t *T) {}

type T struct{}

// A method is not a package-level function, so it is not a test declaration
// even with the exact parameter shape.
type Suite struct{}

func (s Suite) TestMethod(t *T) {}

func (s *Suite) TestPointerMethod(t *T) {}
