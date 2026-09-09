package benchmarks

import "testing"

// Benchmarks and fuzz targets are the admitted variation of the one
// go.testing.test_function family: the same declaration shape with the B and
// F parameter types. The bare exported name Test is also what `go test` runs,
// because the anchor mirrors the toolchain's own name rule exactly.

func BenchmarkSorts(b *testing.B) {
	for i := 0; i < b.N; i++ {
	}
}

func BenchmarkParses(b *testing.B) {
	b.ResetTimer()
}

func FuzzParses(f *testing.F) {
	f.Add("seed")
	f.Fuzz(func(t *testing.T, raw string) {
		if raw == "" {
			t.Skip()
		}
	})
}

func Test(t *testing.T) {}
