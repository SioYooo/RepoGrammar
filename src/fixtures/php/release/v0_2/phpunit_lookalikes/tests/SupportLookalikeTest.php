<?php

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\TestCase;

function testHelperFunction(): void
{
    // A test-prefixed free function is not a PHPUnit test method.
}

class SortHelper
{
    public function testCompareEntries(): void
    {
        // A public test-prefixed method in a class that does not derive from
        // a TestCase-suffixed base is a lookalike.
    }
}

abstract class BasePaymentTest extends TestCase
{
    abstract public function testSignatureOnly(): void;

    public function testDeclaredInAbstractBase(): void
    {
        // An abstract base is never instantiated, so its members do not
        // anchor even though the derivation holds.
    }
}

class PaymentTest extends TestCase
{
    private function testHiddenByVisibility(): void
    {
    }

    protected function testAlsoHiddenByVisibility(): void
    {
    }

    public static function testStaticShape(): void
    {
    }
}
