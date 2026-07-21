from pydantic import BaseModel


class Product(BaseModel):
    id: int
    sku: str
    price: float


class Category(BaseModel):
    id: int
    sku: str
    price: float


class Warehouse(BaseModel):
    id: int
    sku: str
    price: float


class Supplier(BaseModel):
    id: int
    sku: str
    price: float


class Shelf(BaseModel):
    id: int
    sku: str
    price: float


