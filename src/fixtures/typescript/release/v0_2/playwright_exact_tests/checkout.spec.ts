import { expect, test } from "@playwright/test";

// Three exact Playwright test cases. `test.describe(...)` is deliberately not
// used here: it is a member call, which this frontend does not anchor.
test("loads the catalog", async ({ page }) => {
  await page.goto("/catalog");
  await expect(page).toHaveTitle("Catalog");
});

test("filters the catalog", async ({ page }) => {
  await page.goto("/catalog?filter=new");
  await expect(page).toHaveTitle("Catalog");
});

test("opens a product", async ({ page }) => {
  await page.goto("/catalog/1");
  await expect(page).toHaveTitle("Product");
});
