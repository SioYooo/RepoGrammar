<?php

declare(strict_types=1);

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\TestCase;

class OrderAnnotationTest extends TestCase
{
    /**
     * @test
     */
    public function appliesVolumeDiscount(): void
    {
        $order = new Order();
        self::assertSame(90, $order->total(100, 0.1));
    }

    /**
     * @test
     */
    public function rejectsUnknownCurrency(): void
    {
        $order = new Order();
        $this->expectException(InvalidArgumentException::class);
        $order->total(100, 0.1, 'PHPUNIT_FIXTURE_SECRET');
    }
}
