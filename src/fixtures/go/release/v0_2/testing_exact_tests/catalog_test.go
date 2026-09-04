package catalog

import "testing"

// Three exact testing declarations, enough to reach Go's minimum family support
// of three. Both admitted parameter spellings appear. Benchmarks and fuzz
// targets are the same family's variation and live in their own fixture.

func TestLoadsCatalog(t *testing.T) {
	t.Log("loads")
}

func TestFiltersCatalog(t *testing.T) {
	t.Log("filters")
}

func TestOpensProduct(*testing.T) {
}

// Not a test: the package entry point and an ordinary helper.
func TestMain(m *testing.M) {
	m.Run()
}

func helper() string {
	return "helper"
}
