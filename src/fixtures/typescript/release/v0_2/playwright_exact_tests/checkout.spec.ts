import { expect, test } from "@playwright/test";

// Real Playwright suites are written as a member call on the imported runner.
// The scanner must not read that as a bare ambient describe: it belongs to the
// Playwright object, not to a jest/vitest global.
test.describe("catalog", () => {
  test("loads the catalog", async ({ page }) => {
    await page.goto("/catalog");
    await expect(page).toHaveTitle("Catalog");
  });

  test("filters the catalog", async ({ page }) => {
    await page.goto("/catalog?filter=new");
    await expect(page).toHaveTitle("Catalog");
  });
});

test("opens a product", async ({ page }) => {
  await page.goto("/catalog/1");
  await expect(page).toHaveTitle("Product");
});
