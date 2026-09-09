using FluentValidation;

namespace Example.Catalog.Validators;

// Three exact FluentValidation validators, enough to reach C#'s minimum family
// support of three. The generic argument is irrelevant to the anchor: the base
// name and the using are what make it exact.
public class OrderValidator : AbstractValidator<Order>
{
    public OrderValidator()
    {
        RuleFor(order => order.Id).NotEmpty();
    }
}

public class CustomerValidator : AbstractValidator<Customer>
{
    public CustomerValidator()
    {
        RuleFor(customer => customer.Email).NotEmpty();
    }
}

public class ShipmentValidator : AbstractValidator<Shipment>
{
    public ShipmentValidator()
    {
        RuleFor(shipment => shipment.Carrier).NotEmpty();
    }
}

public class Order
{
    public string Id { get; set; } = string.Empty;
}

public class Customer
{
    public string Email { get; set; } = string.Empty;
}

public class Shipment
{
    public string Carrier { get; set; } = string.Empty;
}
