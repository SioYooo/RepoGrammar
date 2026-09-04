<?php

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\TestCase;

class DegradedCloseTagTest extends TestCase
{
    public function testBeforeCloseTag(): void
    {
        self::assertTrue(true);
    }
}
?>
<p>Inline HTML after the close tag reopens inline-output mode, which is
outside the declared subset, so the whole file abstains.</p>
