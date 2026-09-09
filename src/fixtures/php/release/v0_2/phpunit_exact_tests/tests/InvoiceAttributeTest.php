<?php

declare(strict_types=1);

namespace Acme\Catalog\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

class InvoiceTestCase extends TestCase
{
    protected function makeInvoice(): Invoice
    {
        return new Invoice();
    }
}

final class InvoiceAttributeTest extends InvoiceTestCase
{
    #[Test]
    public function totalsNetAmount(): void
    {
        $invoice = $this->makeInvoice();
        self::assertSame(0, $invoice->net());
    }
}
