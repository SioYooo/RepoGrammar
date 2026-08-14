namespace Example.Lookalikes.Validators;

// A locally declared type that merely shares the FluentValidation base name.
// Without `using FluentValidation;` and without the fully qualified path, it is
// some other AbstractValidator and must never anchor a role.
public class AbstractValidator<T>
{
}

public class Order
{
}

public class Customer
{
}

public class Shipment
{
}

public class OrderValidator : AbstractValidator<Order>
{
}

public class CustomerValidator : AbstractValidator<Customer>
{
}

public class ShipmentValidator : AbstractValidator<Shipment>
{
}
