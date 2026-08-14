# Three admitted top-level test_that blocks, enough to reach R's minimum family
# support of three. The commented and quoted calls below must anchor nothing.

# test_that("commented out", { })
skipped <- "test_that('quoted', { })"

test_that("loads the catalog", {
  expect_true(TRUE)
})

test_that("filters the catalog", {
  closing_brace <- "}"
  expect_equal(nchar(closing_brace), 1)
})

test_that('opens a product', {
  expect_false(FALSE)
})

# Not admitted: nested, namespaced, and computed-description calls.
local({
  test_that("nested", {
    expect_true(TRUE)
  })
})

testthat::test_that("namespaced", {
  expect_true(TRUE)
})
