from pydantic import BaseModel


class OrderSchema(BaseModel):
    id: int
    total: float
    note: str


class InvoiceSchema(BaseModel):
    id: int
    total: float
    note: str


class PaymentSchema(BaseModel):
    id: int
    total: float
    note: str


class ShipmentSchema(BaseModel):
    id: int
    total: float
    note: str


class RefundSchema(BaseModel):
    id: int
    total: float
    note: str


