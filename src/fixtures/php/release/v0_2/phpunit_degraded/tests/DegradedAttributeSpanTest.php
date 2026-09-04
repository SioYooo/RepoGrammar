<?php

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\Attributes\TestWith;
use PHPUnit\Framework\TestCase;

class DegradedAttributeSpanTest extends TestCase
{
    #[Test]
    public function testBeforeSpanningAttribute(): void
    {
        self::assertTrue(true);
    }

    #[TestWith([
        'amount' => 1,
    ])]
    public function testSpansMultipleLines(int $amount): void
    {
        // An attribute that runs past its opening line is where the PHP 7.4
        // comment reading and the PHP 8.x attribute reading disagree about
        // token boundaries, so the whole file abstains.
    }
}
