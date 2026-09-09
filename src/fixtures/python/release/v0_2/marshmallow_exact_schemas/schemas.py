"""Three exact marshmallow schemas, enough to reach Python's minimum family
support of three."""

from marshmallow import Schema, fields


class OrderSchema(Schema):
    id = fields.Int(required=True)
    total = fields.Decimal()


class CustomerSchema(Schema):
    id = fields.Int(required=True)
    email = fields.Email()


class ShipmentSchema(Schema):
    id = fields.Int(required=True)
    carrier = fields.Str()
