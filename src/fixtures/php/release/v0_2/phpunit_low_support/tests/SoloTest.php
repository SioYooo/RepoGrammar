<?php

// Bounded PHPUnit low-support fixture: two anchors, one below the family
// minimum of three.

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\TestCase;

class SoloTest extends TestCase
{
    public function testRunsSolo(): void
    {
        self::assertTrue(true);
    }

    public function testRunsSoloAgain(): void
    {
        self::assertTrue(true);
    }
}
