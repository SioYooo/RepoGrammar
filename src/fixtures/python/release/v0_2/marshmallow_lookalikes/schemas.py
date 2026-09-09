"""A locally defined Schema base shares the name and nothing else. Without the
marshmallow import these classes must anchor no framework role."""


class Schema:
    pass


class OrderSchema(Schema):
    pass


class CustomerSchema(Schema):
    pass


class ShipmentSchema(Schema):
    pass
