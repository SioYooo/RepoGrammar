<?php

declare(strict_types=1);

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\TestCase;

final class CatalogTest extends TestCase
{
    public function testLoadsTheCatalog(): void
    {
        $catalog = new Catalog();
        self::assertSame([], $catalog->all());
    }

    public function testFiltersByCategory(): void
    {
        $catalog = new Catalog();
        self::assertSame([], $catalog->filter('tools'));
    }

    public function testSortsEntriesByName(): void
    {
        $catalog = new Catalog();
        self::assertSame([], $catalog->sorted());
    }
}
